use std::sync::Arc;

use bounded_integer::BoundedUsize;
use btleplug::api::Peripheral as _;
use futures_util::{stream, StreamExt};
use tracing::{info, Instrument, Span};

use crate::inner::countdown_latch::CountDownLatch;
use crate::inner::dto::{
    IoCommand, PeripheralIoBatchRequestDto, PeripheralIoBatchResponseDto, PeripheralIoRequestDto,
    PeripheralIoResponseDto, ResultDto,
};
use crate::inner::error::{CollectorError, CollectorResult};
use crate::inner::peripheral_manager::PeripheralManager;

impl PeripheralIoBatchRequestDto {
    fn get_async_reads_count(&self) -> usize {
        self.commands
            .iter()
            .filter(|cmd| {
                matches!(cmd, IoCommand::Read {
                    wait_notification, ..
                } if *wait_notification)
            })
            .count()
    }
}

#[tracing::instrument(level = "info", skip_all)]
pub(crate) async fn execute_batches(
    peripheral_manager: Arc<PeripheralManager>,
    request: PeripheralIoRequestDto,
) -> PeripheralIoResponseDto {
    let manager_stream = std::iter::repeat_with(|| Arc::clone(&peripheral_manager));
    let span = Span::current();
    let batch_responses = stream::iter(request.batches.into_iter().zip(manager_stream))
        .map(|(batch, peripheral_manager)| async { execute_batch(peripheral_manager, batch, span.clone()).await })
        .buffered(
            request
                .parallelism
                .map(BoundedUsize::get)
                .unwrap_or(peripheral_manager.app_conf.default_multi_batch_parallelism),
        )
        .collect::<Vec<_>>()
        .in_current_span()
        .await;

    PeripheralIoResponseDto { batch_responses }
}

#[tracing::instrument(level = "info", skip_all, parent = &_parent_span)]
async fn execute_batch(
    peripheral_manager: Arc<PeripheralManager>,
    batch: PeripheralIoBatchRequestDto,
    _parent_span: Span,
) -> PeripheralIoBatchResponseDto {
    let latch = Arc::new(CountDownLatch::new(batch.get_async_reads_count()));
    let manager_stream = std::iter::repeat_with(|| Arc::clone(&peripheral_manager));
    let latch_stream = std::iter::repeat_with(|| Arc::clone(&latch));

    let span = Span::current();

    let command_responses: Vec<Option<ResultDto<Vec<u8>>>> =
        stream::iter(batch.commands.into_iter().zip(manager_stream).zip(latch_stream))
            .map(|((cmd, manager), latch)| async {
                let span = span.clone();
                match cmd {
                    IoCommand::Read { .. } => {
                        let read_result = read_value_with_timeout(manager, latch, cmd, span).await;
                        Some(read_result.into())
                    }
                    IoCommand::Write { .. } => {
                        if let Err(err) = write_value_with_timeout(manager, latch, cmd, span).await {
                            Some(Err(err).into())
                        } else {
                            None
                        }
                    }
                }
            })
            .buffered(
                batch
                    .parallelism
                    .map(BoundedUsize::get)
                    .unwrap_or(peripheral_manager.app_conf.default_batch_parallelism),
            )
            .collect::<Vec<_>>()
            .await;

    PeripheralIoBatchResponseDto { command_responses }
}

#[tracing::instrument(level = "info", skip_all, parent = &_parent_span, err, fields(
    peripheral = %cmd.get_fqcn().peripheral,
    service = %cmd.get_fqcn().service,
    characteristic = %cmd.get_fqcn().characteristic,
    timeout = ?cmd.get_timeout(),
))]
async fn read_value_with_timeout(
    manager: Arc<PeripheralManager>,
    latch: Arc<CountDownLatch>,
    cmd: IoCommand,
    _parent_span: Span,
) -> CollectorResult<Vec<u8>> {
    let timeout_duration = cmd.get_timeout().unwrap_or(manager.app_conf.default_read_timeout);
    let result = tokio::time::timeout(timeout_duration, read_value(manager, latch, cmd)).await??;
    Ok(result)
}

async fn read_value(
    manager: Arc<PeripheralManager>,
    latch: Arc<CountDownLatch>,
    cmd: IoCommand,
) -> CollectorResult<Vec<u8>> {
    let IoCommand::Read {
        fqcn,
        wait_notification,
        timeout_ms,
    } = cmd
    else {
        return Err(CollectorError::UnexpectedIoCommand);
    };

    info!("Reading value");

    let (peripheral, characteristic) = manager.get_peripheral_characteristic(&fqcn).await?;

    if !wait_notification {
        let value = peripheral.read(&characteristic).await?;
        manager.disconnect_if_has_no_tasks(peripheral).await?;
        return Ok(value);
    }

    peripheral.subscribe(&characteristic).await?;
    let mut notification_stream = peripheral.notifications().await?;
    let result = tokio::spawn(async move {
        latch.countdown();
        while let Some(event) = notification_stream.next().await {
            if !fqcn.matches(&event) {
                continue;
            }
            return Ok(event.value);
        }
        Err(CollectorError::EndOfStream)
    });

    let timeout_duration = timeout_ms.unwrap_or(manager.app_conf.default_read_timeout);

    let result = tokio::time::timeout(timeout_duration, result).await??;
    let _ = manager.disconnect_if_has_no_tasks(peripheral).await;
    result
}

#[tracing::instrument(level = "info", skip_all, parent = &_parent_span, err, fields(
    peripheral = %cmd.get_fqcn().peripheral,
    service = %cmd.get_fqcn().service,
    characteristic = %cmd.get_fqcn().characteristic,
))]
async fn write_value_with_timeout(
    manager: Arc<PeripheralManager>,
    latch: Arc<CountDownLatch>,
    cmd: IoCommand,
    _parent_span: Span,
) -> CollectorResult<()> {
    let timeout_duration = cmd.get_timeout().unwrap_or(manager.app_conf.default_write_timeout);
    tokio::time::timeout(timeout_duration, write_value(manager, latch, cmd)).await??;
    Ok(())
}

async fn write_value(
    manager: Arc<PeripheralManager>,
    latch: Arc<CountDownLatch>,
    cmd: IoCommand,
) -> CollectorResult<()> {
    let write_type = cmd.get_write_type();
    let IoCommand::Write {
        fqcn,
        value,
        wait_response: _,
        ..
    } = cmd
    else {
        return Err(CollectorError::UnexpectedIoCommand);
    };

    info!("Writing value");

    let (peripheral, characteristic) = manager.get_peripheral_characteristic(&fqcn).await?;

    latch.wait().await;

    let result = peripheral.write(&characteristic, &value, write_type).await;

    manager.disconnect_if_has_no_tasks(peripheral).await?;

    result?;
    Ok(())
}

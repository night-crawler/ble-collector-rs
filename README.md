# BLE Collector RS

## Overview
It's a Rust-based project for collecting BLE (Bluetooth Low Energy) data.

## Features
- Parallel data collection from BLE peripherals
- Support for characteristic notifications and polling
- REST API proxy for reading and writing BLE characteristics
- [GATT Specification Supplement](https://btprodspecificationrefs.blob.core.windows.net/gatt-specification-supplement/GATT_Specification_Supplement.pdf) data converter (convert values like `Represented values: M = 1, d = -2, b = 0`)
- Scan/discovery data is available over HTTP
- Match devices for collection by name or MAC address using contains / equal / startswith / regex.

For a sample configuration file, see [example.yaml](example.yaml).

The easiest configuration sample:

```yaml 

peripherals:
  - name: 'Sensor Hub'
    device_name: !StartsWith 'Sensor Hub'
    # device_id: !Equals 'FA:6F:EC:EE:4B:36'
    services:
      - uuid: '0000180a-0000-1000-8000-00805f9b34fb'
        name: 'Device Information'
        default_delay: 60s
        default_history_size: 10
        characteristics:
          - !Subscribe
            name: 'Battery Voltage'
            uuid: '00002b18-0000-1000-8999-00805f9b34fb'
            converter: !Unsigned { l: 2, m: 1, d: 0, b: -6 }
```

## URLS

```bash
curl -v http://localhost:8000/ble/data | jq
curl -v http://localhost:8000/ble/adapters | jq
curl -v http://localhost:8000/ble/adapters/describe | jq

# Read / write characteristics using endpoint
http://localhost:8000/ble/adapters/hci0/rw 
```

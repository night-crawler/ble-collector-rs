# BLE Collector RS

## Overview

It's a Rust-based project for collecting, processing and exporting BLE (Bluetooth Low Energy) data.

## Features

### Bluetooth Low Energy

- Multiple adapters support (you can specify adapter name like `hci0`)
- Parallel data collection from BLE peripherals
- Support for characteristic notifications and polling (you can specify polling interval)
- [GATT Specification Supplement](https://btprodspecificationrefs.blob.core.windows.net/gatt-specification-supplement/GATT_Specification_Supplement.pdf) data converter (convert values like `Represented values: M = 1, d = -2, b = 0`)
- Match devices for collection by name or MAC address using contains / equal / startswith / regex.

### HTTP

- Scan/discovery data is available over HTTP
- REST API proxy for reading and writing BLE characteristics (you can specify r/w batch parallelism)

### MQTT

- Export characteristics to MQTT (state topic + discovery topic)
- Templating support for discovery topic names and the whole discovery config section (use [rhai](https://github.com/rhaiscript/rhai) scripting language) <sup>1</sup>

The whole `discovery` config section is optional, so you can use only state topic. Also, it can contain free-form data.

<sup>1</sup> 
- You can use `ctx` variable to access the context of the current payload (e.g. `ctx.fqcn.peripheral`)
- At the moment all values from the discovery section are treated as rhai scripts, so every literal must be a valid rhai
    expression (e.g. '`voltage`' is valid, but 'voltage' is not). A good way to solve it would be having tagged YAML
    literals, but it's not supported by serde_yaml at the moment (https://github.com/dtolnay/serde-yaml/issues/395).

### Metrics

- Treat characteristics as metrics and export them to Prometheus (`/metrics` endpoint)
- Observe collector stats (e.g. number of connected devices, number of characteristics, etc.)

For a sample configuration file, see [example.yaml](example.yaml).

The easiest configuration sample:

```yaml 

peripherals:
  - name: 'Sensor Hub'
    device_name: !StartsWith 'Sensor Hub'  # match by device name
    adapter: !Equals 'hci0'  # and by adapter name
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
            publish_metrics:
              metric_type: Gauge
              name: 'sensor_hub_device_information_battery_voltage_volts'
              description: 'Sensor Hub Battery Voltage'
              unit: 'Volt'
              labels:
                - [ 'scope', 'device' ]
            publish_mqtt:
              state_topic: '`sensor_hub/${ctx.fqcn.peripheral}/device_information/battery_voltage`'
              discovery:
                config_topic: '`homeassistant/sensor/sensor_hub_${ctx.clean_fqcn.peripheral}_device_information_battery_voltage/config`'
                state_topic: '`${state_topic}`'
                device_class: '`voltage`'
                unit_of_measurement: '`V`'
                value_template: '`{{ value_json.value }}`'
                unique_id: '`${ctx.clean_fqcn.peripheral}`'
                device:
                  identifiers:
                  - '`${ ctx.clean_fqcn.peripheral }`'
                  name: >-
                    switch ctx.fqcn.peripheral {
                      "FA:6F:EC:EE:4B:36" => "Sensor Hub Living Room",
                      "D4:B7:67:56:DC:3B" => "Sensor Hub Kitchen",
                      "D0:F6:3B:34:4C:1F" => "Sensor Hub Bedroom",
                      _ => "Sensor Hub ${fqcn.peripheral}"
                    }
```

## URLS

```bash
curl -v http://localhost:8000/ble/data | jq
curl -v http://localhost:8000/ble/adapters | jq
curl -v http://localhost:8000/ble/adapters/describe | jq

# Read / write characteristics using endpoint
http://localhost:8000/ble/adapters/hci0/rw 
```

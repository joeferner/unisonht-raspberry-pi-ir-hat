# Mock Serial Port

```
socat -d -d pty,raw,echo=0 pty,raw,echo=0

2021/09/19 02:08:25 socat[145655] N PTY is /dev/pts/6   <== listen to this end
2021/09/19 02:08:25 socat[145655] N PTY is /dev/pts/7   <== write to this end
2021/09/19 02:08:25 socat[145655] N starting data transfer loop with FDs [5,5] and [7,7]

MQTT_URI=tcp://localhost:1883 HAT_PORT=/dev/pts/6 HAT_CONFIG=example.config.yaml cargo run
printf '!s100\n!s200\n!s300\n' > /dev/pts/7
```

# Testing

```
docker run --rm -it --name mosquitto -p 1883:1883 -p 9001:9001 --mount type=bind,source="$(pwd)"/mosquitto.conf,target=/mosquitto/config/mosquitto.conf eclipse-mosquitto
docker exec -it mosquitto mosquitto_sub -t ir/#
docker exec -it mosquitto mosquitto_pub -t ir/tx -m '{"remote_name":"test1","button_name":"volumeUp"}'
HAT_CONFIG=~/pi-ir.yaml MQTT_URI=tcp://192.168.68.121:1883 ./target/debug/unison-raspberry-pi-ir-hat
```

# Building on Raspberry Pi

```
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
git clone git@github.com:joeferner/raspberry-pi-ir-hat.git
cd raspberry-pi-ir-hat/drivers/rust/
sudo apt-get install libudev-dev
cargo build
cd ../../..
git clone git@github.com:joeferner/unisonht-raspberry-pi-ir-hat.git
cd unisonht-raspberry-pi-ir-hat/
sudo apt-get install libssl-dev cmake
cargo build
```

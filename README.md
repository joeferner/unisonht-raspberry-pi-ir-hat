# Mock Serial Port

```
socat -d -d pty,raw,echo=0 pty,raw,echo=0

2021/09/19 02:08:25 socat[145655] N PTY is /dev/pts/6   <== listen to this end
2021/09/19 02:08:25 socat[145655] N PTY is /dev/pts/7   <== write to this end
2021/09/19 02:08:25 socat[145655] N starting data transfer loop with FDs [5,5] and [7,7]

cargo run -- --file test.json --port /dev/pts/6
echo '!s123' > /dev/pts/7
```

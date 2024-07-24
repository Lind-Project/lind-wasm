First you need to download lind-wasm in your docker to home directory

```
cd /home
sudo git clone --recurse-submodules https://github.com/Lind-Project/lind-wasm.git
```

I assume you have rust else use
```
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs/ | sh
rustup install nightly
. "$HOME/.cargo/env"
rustup default nightly
```

Run set.sh file
```
cd /home/lind-wasm
./set.sh
```

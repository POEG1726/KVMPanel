# kvmd

在 Linux/WSL 中使用 Rust stable。初始账号为 `admin`，密码为 `admin`，首次登录返回 `must_change_password`。在仓库根目录运行以下命令，HTTP 监听 127.0.0.1:8800。

```sh
. "$HOME/.cargo/env"
export CARGO_TARGET_DIR="$HOME/work/KVMPanel/build/kvmd-cargo"
cargo run --manifest-path software/app/ipkvm/Cargo.toml -- --sim \
  --listen 127.0.0.1:8800 --data-dir "$HOME/work/KVMPanel/build/kvmd-sim" \
  --web-dir software/app/web/dist
```

默认板端数据目录为 `/data/kvmd/`。账号密码使用 SHA-256 预散列以保留长密码的所有字节，随后使用独立盐值做 bcrypt。配置和账号通过同一目录的临时文件、fsync 和 rename 持久化。会话只保存在进程内。构建时从相邻的 API 目录读取 OpenAPI 和消息 schema，板上无需安装 YAML 文件。

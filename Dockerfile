# 使用官方Rust镜像作为构建环境
FROM --platform=$BUILDPLATFORM rust:1.81.0-slim-bookworm AS builder


# 设置工作目录
WORKDIR /usr/src/app

# 复制Cargo.toml和Cargo.lock文件
COPY Cargo.toml Cargo.lock ./

# 创建一个虚拟的main.rs文件,用于缓存依赖
RUN mkdir src && echo "fn main() {}" > src/main.rs

# 构建依赖
RUN cargo build --release

# 删除虚拟的main.rs文件
RUN rm -f src/main.rs

# 复制实际的源代码
COPY src ./src
COPY templates ./templates

# 构建实际的应用
RUN cargo build --release

# 安装SSL证书和SQLite3
RUN apt-get update && apt-get install -y ca-certificates sqlite3 libsqlite3-0 && rm -rf /var/lib/apt/lists/*

# 从构建阶段复制编译好的二进制文件
FROM --platform=$TARGETPLATFORM debian:bookworm-slim

# 复制模板文件
COPY --from=builder /usr/src/app/templates /usr/local/bin/templates
COPY --from=builder /usr/src/app/target/release/class7-practice /usr/local/bin/class7-practice

# 创建数据目录
RUN mkdir /data

COPY local.db /var/local.db

# 设置工作目录
WORKDIR /usr/local/bin

# 暴露3000端口
EXPOSE 3000

# 设置环境变量指定数据库路径
ENV DATABASE_URL=/var/local.db

# 运行应用
CMD ["./class7-practice"]
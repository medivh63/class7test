# 构建阶段
FROM rust:1.76 as builder
WORKDIR /usr/src/app
COPY . .
RUN cargo build --release

# 运行阶段
FROM debian:bullseye-slim
RUN apt-get update && apt-get install -y libsqlite3-0 && rm -rf /var/lib/apt/lists/*
COPY --from=builder /usr/src/app/target/release/class7test /usr/local/bin/app
COPY --from=builder /usr/src/app/templates /templates
COPY --from=builder /usr/src/app/.env /.env

EXPOSE 8080
CMD ["app"]
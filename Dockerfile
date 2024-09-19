# 使用多阶段构建
# 第一阶段: 构建阶段
FROM --platform=$BUILDPLATFORM rust:1.81 as builder

WORKDIR /usr/src/app

# 复制项目文件
COPY . .

# 设置SQLX_OFFLINE为true，以确保在离线模式下构建

# 设置SQLX_OFFLINE环境变量
ENV SQLX_OFFLINE=true

# 构建项目
RUN cargo build --release

# 第二阶段: 运行阶段
FROM --platform=$TARGETPLATFORM ubuntu:22.04

# 安装必要的运行时依赖
RUN apt-get update && apt-get install -y libsqlite3-0 && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# 从构建阶段复制编译好的二进制文件
COPY --from=builder /usr/src/app/target/release/class7-practice .
# 复制templates
COPY --from=builder /usr/src/app/templates /app/templates


# 创建一个目录用于挂载SQLite数据库
RUN mkdir /data

# 设置环境变量
ENV DATABASE_URL=/data/local.db

# 暴露应用端口（根据您的应用需要调整）
EXPOSE 3000

# 运行应用
CMD ["./class7-practice"]

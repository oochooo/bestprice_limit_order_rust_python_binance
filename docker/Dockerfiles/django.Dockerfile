FROM rust:1.80.1 AS rust
COPY cli/trader-cli /trader-cli
WORKDIR /trader-cli
RUN PYO3_PYTHON=python3.11 cargo build --release


FROM python:3.11
ENV PYTHONDONTWRITEBYTECODE 1
ENV PYTHONUNBUFFERED 1
WORKDIR /app
COPY --from=rust /trader-cli/target/release/librust_trader.so /app/rust_trader.so
COPY main/requirements.txt /app
RUN pip install -r requirements.txt

COPY ./main/ .


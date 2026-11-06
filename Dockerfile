FROM --platform=$BUILDPLATFORM oven/bun:1 AS web
WORKDIR /web
COPY web/package.json web/bun.lock ./
RUN bun install --frozen-lockfile
COPY web/ ./
# vite.config.ts reads the app version from ../Cargo.toml.
COPY Cargo.toml /Cargo.toml
RUN bun run build

# Built natively per target platform (QEMU for the foreign arch: slow, tens of minutes).
FROM rust:1-alpine AS build
RUN apk add --no-cache build-base
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY migrations ./migrations
COPY --from=web /web/build ./web/build
RUN cargo build --release --locked

FROM alpine AS data
RUN mkdir /data && chown 65532:65532 /data

FROM gcr.io/distroless/static-debian12:nonroot
LABEL org.opencontainers.image.title="tiiny" \
      org.opencontainers.image.description="Self-hosted link shortener that counts clicks, not people. Custom slugs, privacy-friendly stats and an admin panel in one small binary." \
      org.opencontainers.image.authors="Adnan Ahmad <viperadnan@gmail.com>" \
      org.opencontainers.image.url="https://github.com/viperadnan-git/tiiny" \
      org.opencontainers.image.source="https://github.com/viperadnan-git/tiiny" \
      org.opencontainers.image.licenses="MIT"
COPY --from=build /src/target/release/tiiny /tiiny
COPY --from=data --chown=65532:65532 /data /data
ENV TIINY_DATA_DIR=/data
VOLUME /data
EXPOSE 8080
ENTRYPOINT ["/tiiny"]

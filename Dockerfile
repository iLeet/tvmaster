# TVMaster with FFmpeg, for a Linux server or NAS.
#   docker build -t tvmaster .
#   docker compose up -d            (see docker-compose.yml)
# Build and run notes are in docs/DOCKER.md.
FROM rust:1-slim-bookworm AS build
WORKDIR /src
COPY . .
RUN cargo build --release --locked -p tvmaster-cli \
 && cp target/release/tvmaster /tvmaster

FROM debian:bookworm-slim
ARG VERSION=dev
LABEL org.opencontainers.image.title="TVMaster" \
      org.opencontainers.image.description="Self-hosted retro TV station and IPTV engine" \
      org.opencontainers.image.version="${VERSION}" \
      org.opencontainers.image.source="https://github.com/iLeet/tvmaster"
RUN apt-get update \
 && apt-get install -y --no-install-recommends ffmpeg fonts-dejavu-core ca-certificates curl util-linux tini \
 && rm -rf /var/lib/apt/lists/*
COPY --from=build /tvmaster /usr/local/bin/tvmaster
COPY docker-entrypoint.sh /usr/local/bin/docker-entrypoint.sh
ENV TVMASTER_DATA_DIR=/data HOME=/data
VOLUME /data
EXPOSE 8787 8788
HEALTHCHECK --interval=30s --timeout=5s --start-period=15s \
  CMD curl -fsS http://127.0.0.1:8787/api/v1/health || exit 1
ENTRYPOINT ["/usr/bin/tini", "--", "/usr/local/bin/docker-entrypoint.sh"]
CMD ["serve", "--bind", "0.0.0.0:8787"]

# TVMaster daemon with FFmpeg, for a Linux server or NAS.
#   docker build -t tvmaster .
#   docker run -d --name tvmaster -p 8787:8787 -v tvmaster-data:/data \
#     -v /path/to/media:/media:ro -e TVMASTER_TOKEN=choose-a-long-token tvmaster
FROM rust:1-slim-bookworm AS build
WORKDIR /src
COPY . .
RUN cargo build --release --locked -p tvmaster-cli

FROM debian:bookworm-slim
RUN apt-get update \
 && apt-get install -y --no-install-recommends ffmpeg fonts-dejavu-core ca-certificates curl \
 && rm -rf /var/lib/apt/lists/* \
 && useradd --system --create-home --home-dir /data tvmaster
COPY --from=build /src/target/release/tvmaster /usr/local/bin/tvmaster
USER tvmaster
ENV TVMASTER_DATA_DIR=/data
VOLUME /data
EXPOSE 8787
HEALTHCHECK --interval=30s --timeout=5s --start-period=10s \
  CMD curl -fsS http://127.0.0.1:8787/api/v1/health || exit 1
ENTRYPOINT ["tvmaster"]
CMD ["serve", "--bind", "0.0.0.0:8787"]

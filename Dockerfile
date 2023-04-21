FROM rust:1.69-bullseye

COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY tests ./tests

# Set up containerized systemd
RUN apt-get update && \
  apt-get install -y --no-install-recommends \
  systemd systemd-sysv dbus dbus-user-session libdbus-1-dev

COPY docker-entrypoint.sh /

ENTRYPOINT ["/docker-entrypoint.sh"]

# Build test binary now so it doesn't need to be built during actual testing
RUN cargo test --no-run

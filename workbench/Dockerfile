FROM rust as builder

RUN apt-get update && \
    apt-get install -y pkg-config libdbus-1-dev systemd systemd-sysv && \
    apt-get clean && rm -rf /var/lib/apt/lists/*

WORKDIR /build

COPY workbench/start /start
RUN chmod +x /start

RUN mkdir -p /run/lock
RUN mkdir -p /mnt/boot

VOLUME [ "/sys/fs/cgroup", "/etc/os-config.json", "/mnt/boot/config.json" ]

ENTRYPOINT [ "/start" ]
CMD [ "/sbin/init" ]

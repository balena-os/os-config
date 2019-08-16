#!/bin/bash

NAME="workbench"
IMAGE_NAME="$(docker build -q -t os-config:dev -f workbench/Dockerfile .)"
echo "Image: ${IMAGE_NAME}"

CONTAINER_ID="$(docker ps -a | grep "${NAME}" | awk '{print $1}')"

if [ -z "${CONTAINER_ID}" ]; then
    echo "Starting container..."
    CONTAINER_ID="$(docker run -d -t --rm --privileged \
        --security-opt seccomp=unconfined \
        --cap-add SYS_ADMIN \
        --tmpfs /run \
        --tmpfs /run/lock \
        -v /sys/fs/cgroup:/sys/fs/cgroup:ro \
        -v "$(PWD)/workbench/boot:/mnt/boot" \
        -v "$(PWD)/workbench/os-config.json:/etc/os-config.json" \
        -v "$(PWD):/build" \
        --name "${NAME}" "${IMAGE_NAME}"
    )"
fi

docker exec -it "${CONTAINER_ID}" "$@"

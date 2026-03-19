FROM ubuntu:16.04

ENV DEBIAN_FRONTEND=noninteractive

RUN apt-get update && apt-get install -y \
    gcc \
    libc6-dev \
    && rm -rf /var/lib/apt/lists/*

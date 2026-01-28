FROM debian:stable-slim

RUN apt-get update \
  && apt-get install -y \
  curl \
  unzip \
  ca-certificates \
  --no-install-recommends \
  && apt-get clean \
  && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Install Deno
ENV DENO_INSTALL=/root/.deno
ENV PATH=$DENO_INSTALL/bin:$PATH
RUN curl -fsSL https://deno.land/install.sh | sh -s v2.6.5

# Copy application files
COPY deno.json deno.lock ./
COPY *.ts ./

# Cache dependencies
RUN deno cache --reload main.ts

ENV PORT=8888
EXPOSE 8888

COPY dockerStart.sh .

ENTRYPOINT ["./dockerStart.sh"]

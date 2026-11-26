# Use realm in container

The `docker` workflow pushes the images of this fork to the [GitHub Container Registry](https://ghcr.io):

- `ghcr.io/kaze-kaze/realm2`: standard realm image
- `ghcr.io/kaze-kaze/realm2-slim`: tailored realm image with only TCP+UDP support. Failover is included; `balance`, `proxy` and `transport` are not.

Both hold the static musl build, for amd64 and arm64.

Tags:

| tag | what it is |
| --- | --- |
| `vX.Y.Z` | that release. Use this one in production. |
| `latest` | the latest release |
| `master` | the tip of the master branch. It moves with every merge, released or not. |

The releases up to v3.1.1 were only ever pushed as `master` and have no tag of their own.

## Notice

`ghcr.io/zhboner/realm2` is upstream's image, without failover. It runs a config that has a `failover` table and never fails over: the table is ignored without a word. To see what an image is, ask it: this fork reports 3.x.

```bash
docker run --rm ghcr.io/kaze-kaze/realm2:latest -v
```

You can also build your own image from the released binaries:

```dockerfile
FROM alpine:latest

ARG VERSION="v3.2.1"

WORKDIR /realm

RUN wget https://github.com/kaze-kaze/realm/releases/download/${VERSION}/realm-x86_64-unknown-linux-musl.tar.gz \
  && tar -zxvf realm-x86_64-unknown-linux-musl.tar.gz \
  && cp realm /usr/bin/realm \
  && chmod +x /usr/bin/realm

ENTRYPOINT ["/usr/bin/realm"]
```

**If there are non-official images, be careful and use at your own risk.**

## Docker

```bash
docker run -d -p 9000:9000 ghcr.io/kaze-kaze/realm2:latest -l 0.0.0.0:9000 -r 192.168.233.2:9000
```

Failover, like everything of an endpoint beyond `-l` and `-r`, is set in a config file:

```bash
docker run -d --name realm --restart unless-stopped --network host \
  -v /etc/realm/config.toml:/etc/realm/config.toml:ro \
  ghcr.io/kaze-kaze/realm2:latest -c /etc/realm/config.toml
```

With `log.level = "warn"` and the default `log.output` (stdout), `docker logs realm` shows when a peer goes down or comes back, see [failover](readme.md#endpointfailover-table).

## Docker Swarm (Docker Compose)

```yaml
# ./realm.yml
version: '3'
services:
  port-9000:
    image: ghcr.io/kaze-kaze/realm2:latest
    ports:
      - 9000:9000
    command: -l 0.0.0.0:9000 -r 192.168.233.2:9000
```

```bash
docker-compose -f ./realm.yml -p realm up -d
```

## Kubernetes

```yaml
# ./realm.yml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: realm-demo-deployment
  labels:
    app: realm
  namespace: default
spec:
  replicas: 1
  selector:
    matchLabels:
      app: realm 
  template:
    metadata:
      labels:
        app: realm 
    spec:
      containers:
      - name: realm
        image: ghcr.io/kaze-kaze/realm2:latest
        args:
          - "-l=0.0.0.0:9000"
          - "-r=192.168.233.2:9000"
        ports:
        - containerPort: 9000
        resources:
          requests:
            memory: "64Mi"
            cpu: "250m"
          limits:
            memory: "128Mi"
            cpu: "500m"
---
apiVersion: v1
kind: Service
metadata:
  name: realm-lb
  namespace: default
spec:
  type: LoadBalancer
  selector:
    app: realm
  ports:
    - name: edge
      port: 9000
```

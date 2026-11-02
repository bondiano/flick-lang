# The Conduit server with the client's built files, from the root of the
# example (`just conduit-run` builds both first).
FROM golang:1.23-alpine AS build

WORKDIR /src
COPY server/dist/go/main/ ./
RUN CGO_ENABLED=0 go build -mod=mod -o /conduit .

FROM alpine:3.21

RUN addgroup -S conduit && adduser -S -G conduit conduit
COPY --from=build --chown=conduit:conduit /conduit /usr/local/bin/conduit
COPY --chown=conduit:conduit client/dist /srv/client
ENV STATIC_DIR=/srv/client
USER conduit
EXPOSE 8000
CMD ["/usr/local/bin/conduit"]

FROM golang:1.23-alpine AS build

WORKDIR /src
COPY dist/go/main/ ./
RUN CGO_ENABLED=0 go build -mod=mod -o /conduit .

FROM alpine:3.21

RUN addgroup -S conduit && adduser -S -G conduit conduit
COPY --from=build --chown=conduit:conduit /conduit /usr/local/bin/conduit
USER conduit
EXPOSE 8000
CMD ["/usr/local/bin/conduit"]

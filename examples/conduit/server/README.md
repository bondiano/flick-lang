# Conduit server

From this directory, run `just conduit-run`. It builds the Go target, then
starts the server and PostgreSQL with Docker Compose. The API listens at
`http://127.0.0.1:8000/api`. Stop both containers with Ctrl+C.

PostgreSQL data stays in the `flick-conduit_postgres_data` Docker volume between
runs. Set `JWT_SECRET` in the shell before starting if you want a different
local signing key.

To run Compose directly after building the Go target, use
`docker compose up --build` from this directory.

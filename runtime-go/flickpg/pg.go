// Package flickpg is the PostgreSQL driver of the go target's Database,
// on pgx: a program imports it only when it reaches the Database service,
// so other programs carry no driver.
package flickpg

import (
	"context"
	"errors"
	"strconv"
	"time"

	"flick.app/flickrt"
	"github.com/jackc/pgx/v5/pgconn"
	"github.com/jackc/pgx/v5/pgconn/ctxwatch"
	"github.com/jackc/pgx/v5/pgxpool"
)

func init() { flickrt.RegisterPg(open) }

// cancelWait is how long a cancelled statement may take to stop before
// its connection is cut.
const cancelWait = 5 * time.Second

func open(c flickrt.PgConfig) (flickrt.PgPool, error) {
	cfg, err := pgxpool.ParseConfig(c.URL)
	if err != nil {
		return nil, err
	}
	cfg.MaxConns = int32(c.MaxConns)
	if c.StatementTimeout > 0 {
		// the server ends a statement that runs longer
		cfg.ConnConfig.RuntimeParams["statement_timeout"] = strconv.FormatInt(c.StatementTimeout.Milliseconds(), 10)
	}
	// an interrupted statement is cancelled on the server, through the
	// protocol's CancelRequest, so its backend is free when it ends
	cfg.ConnConfig.BuildContextWatcherHandler = func(conn *pgconn.PgConn) ctxwatch.Handler {
		return &pgconn.CancelRequestContextWatcherHandler{Conn: conn, DeadlineDelay: cancelWait}
	}
	pool, err := pgxpool.NewWithConfig(context.Background(), cfg)
	if err != nil {
		return nil, err
	}
	return &pgPool{pool, c.AcquireTimeout}, nil
}

type pgPool struct {
	pool    *pgxpool.Pool
	timeout time.Duration
}

func (p *pgPool) Acquire(ctx context.Context) (flickrt.PgConn, error) {
	if p.timeout > 0 {
		var cancel context.CancelFunc
		ctx, cancel = context.WithTimeout(ctx, p.timeout)
		defer cancel()
	}
	conn, err := p.pool.Acquire(ctx)
	if err != nil {
		return nil, failure(err)
	}
	return &pgConn{conn}, nil
}

func (p *pgPool) Close() { p.pool.Close() }

type pgConn struct{ conn *pgxpool.Conn }

// Exec runs the statement with text parameters of types the server infers,
// reading every result as text.
func (c *pgConn) Exec(ctx context.Context, text string, args []*string) (*flickrt.PgResult, error) {
	values := make([][]byte, len(args))
	for i, a := range args {
		if a != nil {
			values[i] = []byte(*a)
		}
	}
	// not ResultReader.Read: it keeps the fields only of a result with rows
	rr := c.conn.Conn().PgConn().ExecParams(ctx, text, values, nil, nil, nil)
	out := &flickrt.PgResult{}
	for rr.NextRow() {
		row := make([]*string, len(rr.Values()))
		for i, v := range rr.Values() {
			if v != nil {
				s := string(v)
				row[i] = &s
			}
		}
		out.Rows = append(out.Rows, row)
	}
	for _, f := range rr.FieldDescriptions() {
		out.Fields = append(out.Fields, flickrt.PgField{Name: f.Name, OID: f.DataTypeOID})
	}
	tag, err := rr.Close()
	if err != nil {
		return nil, failure(err)
	}
	out.RowCount = tag.RowsAffected()
	return out, nil
}

// Release hands the connection back, or closes it.
func (c *pgConn) Release(destroy bool) {
	if destroy {
		ctx, cancel := context.WithTimeout(context.Background(), time.Second)
		c.conn.Conn().Close(ctx)
		cancel()
	}
	c.conn.Release()
}

// failure is what the server reported, with its SQLSTATE, or the error.
func failure(err error) error {
	var pg *pgconn.PgError
	if errors.As(err, &pg) {
		return &flickrt.PgError{Code: pg.Code, Message: pg.Message, Constraint: pg.ConstraintName}
	}
	return err
}

package flickrt

// The service `Database.Database` over a pool of PostgreSQL connections. A
// query borrows a connection for its whole run, a transaction for all of
// its queries. An interrupted query is cancelled on the server and its
// connection closed, never handed back to the pool; what it wrote is not
// rolled back, unless it ran in a transaction, which then ends. The pool
// ends with the scope of the `provide`. The driver is the package
// flickpg, which a program imports only when it reaches the service.

import (
	"strconv"
	"sync"
	"time"
)

// PgConfig is `Database.PgConfig`.
type PgConfig struct {
	URL              string
	MaxConns         int
	StatementTimeout time.Duration // 0 for none
	AcquireTimeout   time.Duration // 0 for none
}

// PgPool and PgConn are what the driver gives.
type PgPool interface {
	Acquire(ctx Context) (PgConn, error)
	Close()
}

type PgConn interface {
	// Exec runs the statement with its placeholders' text (nil for NULL),
	// reading the results as text.
	Exec(ctx Context, text string, args []*string) (*PgResult, error)
	// Release hands the connection back, or closes it with `destroy`.
	Release(destroy bool)
}

type PgResult struct {
	Fields   []PgField
	Rows     [][]*string
	RowCount int64
}

type PgField struct {
	Name string
	OID  uint32
}

// PgError is a failure the server reported, with its SQLSTATE.
type PgError struct{ Code, Message, Constraint string }

func (e *PgError) Error() string { return e.Message }

var pgOpen func(PgConfig) (PgPool, error)

// RegisterPg makes `open` the driver's.
func RegisterPg(open func(PgConfig) (PgPool, error)) { pgOpen = open }

func dbError(err error) Value {
	if e, ok := err.(*PgError); ok {
		constraint := None
		if e.Constraint != "" {
			constraint = Some(e.Constraint)
		}
		return NewData(sqlDbErrorTag, Rec("code", e.Code, "message", e.Message, "constraint", constraint))
	}
	return NewData(sqlDbErrorTag, Rec("code", "", "message", err.Error(), "constraint", None))
}

// codeless is whether a failure has no SQLSTATE: one of the connection.
func codeless(err error) bool {
	_, ok := err.(*PgError)
	return !ok
}

// pgExec runs on the connection, interruptibly: an interruption cancels the
// statement on the server, and the connection is then closed.
func pgExec(fb *Fiber, conn PgConn, text string, args []*string, release bool) *PgResult {
	return ForeignAsync(fb, func(ctx Context) Value {
		res, err := conn.Exec(ctx, text, args)
		if release {
			conn.Release(ctx.Err() != nil || err != nil && codeless(err))
		}
		if err != nil {
			return Fail(dbError(err))
		}
		return res
	}).(*PgResult)
}

func acquire(fb *Fiber, pool PgPool) PgConn {
	return ForeignAsync(fb, func(ctx Context) Value {
		conn, err := pool.Acquire(ctx)
		if err != nil {
			return Fail(dbError(err))
		}
		return conn
	}).(PgConn)
}

// txConn is a transaction's connection: released once, and when a
// statement still runs on it, by that statement as it ends.
type txConn struct {
	PgConn
	mu                  sync.Mutex
	busy, done, destroy bool
}

func (c *txConn) Exec(ctx Context, text string, args []*string) (*PgResult, error) {
	c.mu.Lock()
	c.busy = true
	c.mu.Unlock()
	res, err := c.PgConn.Exec(ctx, text, args)
	c.mu.Lock()
	c.busy = false
	if c.destroy && !c.done {
		c.done = true
		c.PgConn.Release(true)
	}
	c.mu.Unlock()
	return res, err
}

func (c *txConn) Release(destroy bool) {
	c.mu.Lock()
	defer c.mu.Unlock()
	switch {
	case c.done:
	case c.busy:
		c.destroy = true
	default:
		c.done = true
		c.PgConn.Release(destroy)
	}
}

// pgDatabase runs statements: on a borrowed connection each, or on the one
// of a transaction.
type pgDatabase struct {
	pool  PgPool
	conn  PgConn // a transaction's
	depth int
}

func (d *pgDatabase) run(fb *Fiber, text string, args []*string) *PgResult {
	if d.conn != nil {
		return pgExec(fb, d.conn, text, args, false)
	}
	return pgExec(fb, acquire(fb, d.pool), text, args, true)
}

func (d *pgDatabase) query(fb *Fiber, q *sqlQuery) *PgResult {
	return d.run(fb, q.text, sqlArgs(fb, q))
}

// databases are the implementations of the service this adapter made,
// which run transactions.
var databases sync.Map

func (d *pgDatabase) service() *Record {
	impl := Rec(
		"run_query", Fn(func(_ *Fiber, q Value) Value {
			return Eff(func(fb *Fiber) Value {
				query := q.(*sqlQuery)
				res := d.query(fb, query)
				if query.template != nil {
					return decodeTyped(fb, query.template, res)
				}
				return decodeRaw(fb, query, res)
			})
		}),
		"run_execute", Fn(func(_ *Fiber, q Value) Value {
			return Eff(func(fb *Fiber) Value { return int32(d.query(fb, q.(*sqlQuery)).RowCount) })
		}),
	).(*Record)
	databases.Store(impl, d)
	return impl
}

// decodeTyped is the rows of a typed query, as records of its results.
func decodeTyped(fb *Fiber, template *Record, res *PgResult) Value {
	values := List(template.Get("columns"))
	columns := make([]sqlColumn, len(values))
	for i, v := range values {
		columns[i] = columnOf(template, record(v))
		oid := uint32(1<<32 - 1)
		if i < len(res.Fields) {
			oid = res.Fields[i].OID
		}
		if !acceptsOID(columns[i].kind, oid) {
			shown := "-1"
			if i < len(res.Fields) {
				shown = strconv.FormatUint(uint64(oid), 10)
			}
			Fail(sqlDecodeError(columns[i].name, "expected a "+columns[i].kind+" column, the database sent type "+shown))
		}
	}
	rows := make([]Value, len(res.Rows))
	for r, raw := range res.Rows {
		fields := make([]Value, 0, 2*len(columns))
		for i, c := range columns {
			nullable := record(values[i]).Get("nullable").(bool)
			if raw[i] == nil {
				if !nullable {
					Fail(sqlDecodeError(c.name, "unexpected NULL"))
				}
				fields = append(fields, c.name, None)
				continue
			}
			v, problem := decodeColumn(fb, c, *raw[i])
			if problem != "" {
				Fail(sqlDecodeError(c.name, problem))
			}
			if nullable {
				v = Some(v)
			}
			fields = append(fields, c.name, v)
		}
		rows[r] = Rec(fields...)
	}
	return rows
}

var rawKinds = map[uint32]string{16: "bool", 21: "number", 23: "number", 700: "number", 701: "number", 114: "json", 3802: "json"}

// decodeRaw is the rows of raw SQL, each object through the decoder.
func decodeRaw(fb *Fiber, q *sqlQuery, res *PgResult) Value {
	rows := make([]Value, len(res.Rows))
	for r, raw := range res.Rows {
		fields := make([]Value, 0, 2*len(res.Fields))
		for i, f := range res.Fields {
			var v Value = JsonNull
			if raw[i] != nil {
				kind, ok := rawKinds[f.OID]
				if !ok {
					kind = "text"
				}
				var problem string
				if v, problem = decodeKind(kind, *raw[i]); problem != "" {
					Fail(sqlDecodeError(f.Name, problem))
				}
			}
			fields = append(fields, f.Name, v)
		}
		decoded, issues := codecOf(q.decoder).decode(fb, Rec(fields...))
		if issues != nil {
			Fail(sqlDecodeError("", issuesMessage(issues)))
		}
		rows[r] = decoded
	}
	return rows
}

const databaseKey = "Database.Database"

// transaction runs `body` between `open` and `close` on success, or the
// statements of `undo` on failure; an interruption ends it with its
// connection, which the server rolls back.
func (d *pgDatabase) transaction(fb *Fiber, body Value) Value {
	conn, depth := d.conn, d.depth
	open, close, undo := "BEGIN", "COMMIT", []string{"ROLLBACK"}
	if conn == nil {
		conn = &txConn{PgConn: acquire(fb, d.pool)}
	} else {
		name := "flick_savepoint_" + strconv.Itoa(depth)
		open, close = "SAVEPOINT "+name, "RELEASE SAVEPOINT "+name
		undo = []string{"ROLLBACK TO SAVEPOINT " + name, "RELEASE SAVEPOINT " + name}
	}
	outer := d.conn == nil
	hand := func(destroy bool) {
		if outer {
			conn.Release(destroy)
		}
	}
	pinned := &pgDatabase{pool: d.pool, conn: conn, depth: depth + 1}
	if _, cause := catch(func() Value { return pgExec(fb, conn, open, nil, false) }); cause != nil {
		hand(true)
		panic(&Failure{cause})
	}
	out, cause := catch(func() Value { return withEnv(fb, databaseKey, pinned.service(), body) })
	switch {
	case cause != nil && cause.has(causeInterrupt):
		hand(true)
		panic(&Failure{cause})
	case cause != nil:
		for _, text := range undo {
			if _, failed := catch(func() Value { return pgExec(fb, conn, text, nil, false) }); failed != nil {
				hand(true)
				panic(&Failure{failed})
			}
		}
		hand(false)
		panic(&Failure{cause})
	}
	// the commit waits for its answer: an interruption could not tell whether it happened
	_, failed := catch(func() Value {
		return masked(fb, fb.state.mask+1, func() Value { return pgExec(fb, conn, close, nil, false) })
	})
	if failed != nil {
		hand(true)
		if outer && failed.kind == causeFail {
			e := record(failed.err.(*Data).Fields[0])
			if e.Get("code") == "" {
				panic(&Failure{&Cause{kind: causeFail, err: NewData(sqlDbErrorTag, Rec(
					"code", "commit_unknown", "message", "the commit's outcome is unknown: "+e.Get("message").(string),
					"constraint", e.Get("constraint")))}})
			}
		}
		panic(&Failure{failed})
	}
	hand(false)
	return out
}

// DatabaseTransaction is `Database.transaction body`: on one connection,
// committed when it succeeds and rolled back when it fails; inside
// another, a savepoint.
func DatabaseTransaction(fb *Fiber, body Value) Value {
	impl, ok := fb.env.get(databaseKey)
	if !ok {
		panic(Defect{"Service not found: " + databaseKey})
	}
	d, ok := databases.Load(impl)
	if !ok {
		panic(Defect{"Error: this `Database` runs no transactions"})
	}
	return d.(*pgDatabase).transaction(fb, body)
}

func millis(o Value) time.Duration {
	if ms, ok := some(o); ok {
		return time.Duration(ms.(float64) * float64(time.Millisecond))
	}
	return 0
}

// DatabasePgLayer is `Database.pg config`: a pool built with the layer and
// ended with its scope.
func DatabasePgLayer(key, config Value) Value {
	c := record(config)
	cfg := PgConfig{
		URL:              c.Get("connection_string").(string),
		MaxConns:         int(c.Get("max_connections").(int32)),
		StatementTimeout: millis(c.Get("statement_timeout_ms")),
		AcquireTimeout:   millis(c.Get("acquire_timeout_ms")),
	}
	return memoized(func(fb *Fiber, _ *MemoMap, scope *Owner) Services {
		if pgOpen == nil {
			panic(Defect{"Error: the program has no PostgreSQL driver"})
		}
		pool, err := pgOpen(cfg)
		if err != nil {
			panic(Defect{"Error: " + err.Error()})
		}
		inner := Within(fb, scope)
		inner.owner.add(inner, func(*Fiber, *Cause) Value { pool.Close(); return Unit })
		return Services{key.(string): (&pgDatabase{pool: pool}).service()}
	})
}

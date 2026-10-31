package flickpg

import (
	"context"
	"os"
	"testing"
	"time"

	"flick.app/flickrt"
)

// dsn is a PostgreSQL to test against, from FLICK_PG_TEST_URL; the e2e
// test `flick/tests/sql_postgres.rs` runs programs against one too.
func dsn(t *testing.T) string {
	url := os.Getenv("FLICK_PG_TEST_URL")
	if url == "" {
		t.Skip("FLICK_PG_TEST_URL is not set")
	}
	return url
}

func TestTextResultsAndServerErrors(t *testing.T) {
	pool, err := open(flickrt.PgConfig{URL: dsn(t), MaxConns: 2})
	if err != nil {
		t.Fatal(err)
	}
	defer pool.Close()
	ctx := context.Background()
	conn, err := pool.Acquire(ctx)
	if err != nil {
		t.Fatal(err)
	}
	one, null := "1", (*string)(nil)
	res, err := conn.Exec(ctx, "select $1::int + 1 as n, $2::text as t, 1.50::numeric as d", []*string{&one, null})
	if err != nil {
		t.Fatal(err)
	}
	if len(res.Rows) != 1 || *res.Rows[0][0] != "2" || res.Rows[0][1] != nil || *res.Rows[0][2] != "1.50" {
		t.Fatalf("rows %v", res.Rows)
	}
	if res.Fields[0].Name != "n" || res.Fields[0].OID != 23 || res.Fields[2].OID != 1700 {
		t.Fatalf("fields %v", res.Fields)
	}
	_, err = conn.Exec(ctx, "select 1/0", nil)
	if e, ok := err.(*flickrt.PgError); !ok || e.Code != "22012" {
		t.Fatalf("division by zero gave %#v", err)
	}
	conn.Release(false)
}

// A cancelled statement stops on the server: its backend is free at once.
func TestCancelledStatementStopsOnTheServer(t *testing.T) {
	pool, err := open(flickrt.PgConfig{URL: dsn(t), MaxConns: 2})
	if err != nil {
		t.Fatal(err)
	}
	defer pool.Close()
	conn, err := pool.Acquire(context.Background())
	if err != nil {
		t.Fatal(err)
	}
	ctx, cancel := context.WithTimeout(context.Background(), 200*time.Millisecond)
	defer cancel()
	start := time.Now()
	if _, err := conn.Exec(ctx, "select pg_sleep(30)", nil); err == nil {
		t.Fatal("the statement was not cancelled")
	}
	conn.Release(true)
	if time.Since(start) > 3*time.Second {
		t.Fatalf("cancelling took %v", time.Since(start))
	}
	other, err := pool.Acquire(context.Background())
	if err != nil {
		t.Fatal(err)
	}
	defer other.Release(false)
	time.Sleep(200 * time.Millisecond)
	res, err := other.Exec(context.Background(), "select count(*) from pg_stat_activity where query like 'select pg_sleep(30)%'", nil)
	if err != nil {
		t.Fatal(err)
	}
	if *res.Rows[0][0] != "0" {
		t.Fatalf("%s sleeping statements left", *res.Rows[0][0])
	}
}

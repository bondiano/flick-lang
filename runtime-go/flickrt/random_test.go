package flickrt

import "testing"

// TestIsaacMatchesEffect checks draws 0, 1, 255 and 299 of seeded
// generators against Effect's `Random.withSeed` on Node.
func TestIsaacMatchesEffect(t *testing.T) {
	cases := []struct {
		name string
		seed []float64
		want [4]float64
	}{
		{"42", []float64{42}, [4]float64{0.2272248550901319, 0.8207217622702011, 0.8027334658272113, 0.3460534820272493}},
		{"1.5", []float64{1.5}, [4]float64{0.3359475394676412, 0.9818145376269317, 0.5909532295689585, 0.4210723993819526}},
		{"-7", []float64{-7}, [4]float64{0.232499889487362, 0.20099643052719673, 0.5955816993183429, 0.9835783847275009}},
		{"2^40+3", []float64{1099511627779}, [4]float64{0.27836684260832056, 0.5178125921654085, 0.6505627951425663, 0.919054151031565}},
		{"string", toIntArray("hello, wörld"), [4]float64{0.39755692034125556, 0.7669193362967264, 0.5037649221172286, 0.9537321832524853}},
	}
	for _, c := range cases {
		r := newIsaac(c.seed)
		var got [300]float64
		for i := range got {
			got[i] = r.nextDouble()
		}
		if have := [4]float64{got[0], got[1], got[255], got[299]}; have != c.want {
			t.Errorf("seed %s: %v, want %v", c.name, have, c.want)
		}
	}
}

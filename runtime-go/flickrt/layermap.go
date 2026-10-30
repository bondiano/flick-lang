package flickrt

import "sync"

// LayerMap and LayerRef: layers built on demand, by key or once, shared
// while a scope holds them. Each key has a layer of its own identity, built
// through one memo map with the services of where the map was made, so a
// key held by a preload or a running `provide` is not built again;
// invalidating a key makes the next use build a new one.
//
// shortcut: idleTimeToLive is ignored, so a key nobody holds is released at once; keep idle entries for the duration if a program needs it

type keyed struct {
	mu     sync.Mutex
	make   *Fiber // the fiber the map was made in, for its services
	memo   *MemoMap
	scope  *Owner
	lookup func(fb *Fiber, key Value) *Layer
	keys   []Value
	layers []*Layer
}

func newKeyed(fb *Fiber, lookup func(*Fiber, Value) *Layer) *keyed {
	return &keyed{make: fb, memo: newMemoMap(nil), scope: fb.owner, lookup: lookup}
}

// layer is the layer of `key`, a new one after an invalidation.
func (k *keyed) layer(fb *Fiber, key Value) *Layer {
	k.mu.Lock()
	defer k.mu.Unlock()
	for i, at := range k.keys {
		if Equals(at, key) {
			return k.layers[i]
		}
	}
	// a memo map of its own, so that what the key's layer memoizes is built anew after an invalidation
	inner, memo := k.lookup(fb, key), newMemoMap(nil)
	l := &Layer{func(fb *Fiber, _ *MemoMap, scope *Owner) Services { return inner.build(fb, memo, scope) }}
	k.keys, k.layers = append(k.keys, key), append(k.layers, l)
	return l
}

func (k *keyed) invalidate(key Value) {
	k.mu.Lock()
	defer k.mu.Unlock()
	for i, at := range k.keys {
		if Equals(at, key) {
			k.keys = append(k.keys[:i:i], k.keys[i+1:]...)
			k.layers = append(k.layers[:i:i], k.layers[i+1:]...)
			return
		}
	}
}

// build is the services of `key`, held by `scope`, built in `fb` with the
// services of the map's maker.
func (k *keyed) build(fb *Fiber, key Value, scope *Owner) Services {
	l := k.layer(fb, key)
	inner := *fb
	inner.env = k.make.env
	return k.memo.getOrBuild(&inner, l, scope, l.build)
}

// get is the layer of the services of `key`.
func (k *keyed) get(key Value) Value {
	return &Layer{func(fb *Fiber, _ *MemoMap, scope *Owner) Services { return k.build(fb, key, scope) }}
}

func LayerMapMake(fb *Fiber, lookup, options Value) Value {
	k := newKeyed(fb, func(fb *Fiber, key Value) *Layer { return layerOf(Apply(fb, lookup, key)) })
	if keys, ok := some(record(options).Get("preloadKeys")); ok {
		for _, key := range List(keys) {
			k.build(fb, key, k.scope)
		}
	}
	return k
}

func LayerMapFromRecord(fb *Fiber, layers, options Value) Value {
	pairs := List(layers)
	k := newKeyed(fb, func(fb *Fiber, key Value) *Layer {
		for _, p := range pairs {
			if p.([]Value)[0] == key {
				return layerOf(p.([]Value)[1])
			}
		}
		panic(Defect{"LayerMap has no key " + Format(key)})
	})
	if record(options).Get("preload").(bool) {
		for _, p := range pairs {
			k.build(fb, p.([]Value)[0], k.scope)
		}
	}
	return k
}

func LayerMapGet(key, m Value) Value { return m.(*keyed).get(key) }

func LayerMapInvalidate(_ *Fiber, key, m Value) Value {
	m.(*keyed).invalidate(key)
	return Unit
}

// A LayerRef is a map of one key whose refresh builds the next layer while
// the scope of the ref holds it.
type layerRef struct {
	*keyed
	mu   sync.Mutex
	hold *Owner // what holds the current build for the ref's scope, if any
}

var refKey Value = Unit

func LayerRefMake(fb *Fiber, layer, options Value) Value {
	r := &layerRef{keyed: newKeyed(fb, func(_ *Fiber, _ Value) *Layer { return layerOf(layer) })}
	if record(options).Get("preload").(bool) {
		r.refresh(fb)
	}
	return r
}

// release lets go of the build the ref holds, as a new one replaces it.
func (r *layerRef) release(fb *Fiber, next *Owner) {
	r.mu.Lock()
	old := r.hold
	r.hold = next
	r.mu.Unlock()
	if old != nil {
		if cause := old.close(fb, nil); cause != nil {
			panic(&Failure{cause})
		}
	}
}

func (r *layerRef) refresh(fb *Fiber) {
	r.invalidate(refKey)
	hold := forkScope(fb, r.scope)
	r.build(fb, refKey, hold)
	r.release(fb, hold)
}

func LayerRefGet(ref Value) Value { return ref.(*layerRef).get(refKey) }

func LayerRefInvalidate(fb *Fiber, ref Value) Value {
	r := ref.(*layerRef)
	r.invalidate(refKey)
	r.release(fb, nil)
	return Unit
}

func LayerRefRefresh(fb *Fiber, ref Value) Value {
	ref.(*layerRef).refresh(fb)
	return Unit
}

// Behavioral transport oracle against the pinned Effect implementation.
// Run: node tools/go-rpc/http_flags.mjs
// HTTP body EOF waits for silent handlers; disconnect interrupts them.
import * as Effect from '../../packages/effect-ts/node_modules/effect/dist/Effect.js';
import * as Rpc from '../../packages/effect-ts/node_modules/effect/dist/rpc/Rpc.js';
import * as Group from '../../packages/effect-ts/node_modules/effect/dist/rpc/RpcGroup.js';
import * as Server from '../../packages/effect-ts/node_modules/effect/dist/rpc/RpcServer.js';
import * as Serialization from '../../packages/effect-ts/node_modules/effect/dist/rpc/RpcSerialization.js';
import * as Http from '../../packages/effect-ts/node_modules/effect/dist/http/HttpEffect.js';
const group=Group.make(Rpc.make('wait'));
for(const flag of ['discard','isNotification']) {
 let start,cleanup; const started=new Promise(r=>start=r),gate=new Promise(()=>{}),cleaned=new Promise(r=>cleanup=r);
 await Effect.runPromise(Effect.scoped(Effect.gen(function*(){
 const httpEffect=yield* Server.toHttpEffect(group);
 const handle=Http.toWebHandler(httpEffect);
 const controller=new AbortController();
 const request=new Request('http://localhost/rpc',{method:'POST',body:JSON.stringify({_tag:'Request',id:'1',tag:'wait',payload:null,headers:[],[flag]:true})+'\n',signal:controller.signal});
 yield* Effect.promise(async()=>{
  const response=handle(request); await started;
  let complete=false; const body=response.then(r=>r.text()).then(t=>{complete=true;return t});
  await new Promise(r=>setTimeout(r,50));
  if(complete)throw Error(`${flag}: HTTP body completed before handler`);
  controller.abort();
  await Promise.race([cleaned,new Promise((_,reject)=>setTimeout(()=>reject(Error('disconnect did not clean handler')),1000))]);
  await body.catch(()=>{});
  console.log(JSON.stringify({flag,bodyCompletedBeforeHandler:false,disconnectInterrupted:true}));
 });
 })).pipe(Effect.provide(group.toLayer(Effect.succeed({wait:()=>Effect.ensuring(Effect.andThen(Effect.sync(start),Effect.promise(()=>gate)),Effect.sync(cleanup))}))),Effect.provide(Serialization.layerNdjson)));
}

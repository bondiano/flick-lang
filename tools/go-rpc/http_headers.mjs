// Behavioral input-error oracle against pinned Effect.
// Run: node tools/go-rpc/http_headers.mjs
// A malformed header pair fails its request without poisoning a healthy peer.
import * as Effect from '../../packages/effect-ts/node_modules/effect/dist/Effect.js';
import * as Rpc from '../../packages/effect-ts/node_modules/effect/dist/rpc/Rpc.js';
import * as Group from '../../packages/effect-ts/node_modules/effect/dist/rpc/RpcGroup.js';
import * as Server from '../../packages/effect-ts/node_modules/effect/dist/rpc/RpcServer.js';
import * as Serialization from '../../packages/effect-ts/node_modules/effect/dist/rpc/RpcSerialization.js';
import * as Http from '../../packages/effect-ts/node_modules/effect/dist/http/HttpEffect.js';
const group=Group.make(Rpc.make('call'));
await Effect.runPromise(Effect.scoped(Effect.gen(function*(){
 const handle=Http.toWebHandler(yield* Server.toHttpEffect(group));
 yield* Effect.promise(async()=>{
  const messages=[{_tag:'Request',id:'1',tag:'call',payload:null,headers:[[]]},{_tag:'Request',id:'2',tag:'call',payload:null,headers:[]}];
  const response=await handle(new Request('http://localhost/rpc',{method:'POST',body:messages.map(JSON.stringify).join('\n')+'\n'}));
  const text=await response.text(); const replies=text.trim().split('\n').map(JSON.parse);
  console.log(text);
  if(replies.length!==2 || replies.some(r=>r._tag!=='Exit'))throw Error('request-local exits expected');
  const bad=replies.find(r=>r.requestId==='1'),good=replies.find(r=>r.requestId==='2');
  if(bad.exit._tag!=='Failure'||bad.exit.cause[0].defect!=='Invalid request headers'||good.exit._tag!=='Success')throw Error('unexpected header failure contract');
 });
})).pipe(Effect.provide(group.toLayer(Effect.succeed({call:()=>Effect.void}))),Effect.provide(Serialization.layerNdjson)));

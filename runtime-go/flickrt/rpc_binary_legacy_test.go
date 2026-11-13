package flickrt

import (
	"encoding/hex"
	"testing"
)

// Recovered Effect 4.0.5 envelopes use a different request layout. Keep
// their bytes as independent evidence, and refuse that incompatible peer.
func TestRPCRejectsLegacyEnvelopeLayouts(t *testing.T) {
	vectors := []struct{ name, tree, hex string }{
		{"msg/request", `{"_tag":"Request","id":4,"tag":"double","payload":"02202a","headers":[["x-a","1"]],"traceId":"4368aac12fc270d0c9240b4b2b84539a","spanId":"59203d779b9235d2","sampled":true}`, "5921cc328ab08f52452507071101353932303364373739623932333564320302202a02000802000106646f75626c652101343336386161633132666332373064306339323430623462326238343533396108010603782d610131"},
		{"msg/requestbare", `{"_tag":"Request","id":"12345678901234567890","tag":"ping","payload":"0120","headers":[]}`, "2b21cc328ab08f5245250700020120150131323334353637383930313233343536373839300470696e670100"},
		{"msg/requestnotify", `{"_tag":"Request","id":0,"tag":"t","payload":"0120","headers":[["a","b"],["c",""]],"isNotification":true,"sampled":false}`, "2421cc328ab08f524525070a02012002000002000001740a02040161016203016300020001"},
		{"msg/ack", `{"_tag":"Ack","requestId":"7"}`, "0d21cc328ab08f52452500020137"},
		{"msg/interrupt", `{"_tag":"Interrupt","requestId":9}`, "0d21cc328ab08f52452508020012"},
		{"msg/ping", `{"_tag":"Ping"}`, "0a21cc328ab08f52452503"},
		{"msg/pong", `{"_tag":"Pong"}`, "0a21cc328ab08f52452506"},
		{"msg/eof", `{"_tag":"Eof"}`, "0a21cc328ab08f52452504"},
		{"msg/chunk", `{"_tag":"Chunk","requestId":3,"values":"0720020102023201"}`, "1621cc328ab08f52452505080720020102023201020006"},
		{"msg/exit", `{"_tag":"Exit","requestId":4,"exit":"03200054"}`, "1221cc328ab08f524525010200080403200054"},
		{"msg/defect", `{"_tag":"Defect","defect":"06202262616422"}`, "1221cc328ab08f524525020706202262616422"},
		{"msgs/many", `[{"_tag":"Request","id":4,"tag":"double","payload":"02202a","headers":[["x-a","1"]],"traceId":"4368aac12fc270d0c9240b4b2b84539a","spanId":"59203d779b9235d2","sampled":true},{"_tag":"Request","id":"12345678901234567890","tag":"ping","payload":"0120","headers":[]},{"_tag":"Request","id":0,"tag":"t","payload":"0120","headers":[["a","b"],["c",""]],"isNotification":true,"sampled":false},{"_tag":"Ack","requestId":"7"},{"_tag":"Interrupt","requestId":9},{"_tag":"Ping"},{"_tag":"Pong"},{"_tag":"Eof"},{"_tag":"Chunk","requestId":3,"values":"0720020102023201"},{"_tag":"Exit","requestId":4,"exit":"03200054"},{"_tag":"Defect","defect":"06202262616422"}]`, "5921cc328ab08f52452507071101353932303364373739623932333564320302202a02000802000106646f75626c652101343336386161633132666332373064306339323430623462326238343533396108010603782d6101312b21cc328ab08f5245250700020120150131323334353637383930313233343536373839300470696e6701002421cc328ab08f524525070a02012002000002000001740a020401610162030163000200010d21cc328ab08f524525000201370d21cc328ab08f524525080200120a21cc328ab08f524525030a21cc328ab08f524525060a21cc328ab08f524525041621cc328ab08f524525050807200201020232010200061221cc328ab08f5245250102000804032000541221cc328ab08f524525020706202262616422"},
	}
	for _, vector := range vectors {
		t.Run(vector.name, func(t *testing.T) {
			data, err := hex.DecodeString(vector.hex)
			if err != nil {
				t.Fatal(err)
			}
			if _, err := newBinParser().feed(data); err == nil {
				t.Fatal("accepted incompatible 4.0.5 envelope")
			}
		})
	}
}

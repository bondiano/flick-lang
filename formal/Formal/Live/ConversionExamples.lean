import Formal.SubAdm
import Formal.IndexedRows

namespace Flick

example (S : Sig) (r s : Ty) : LeLive S (.ref (.run r s)) (.ref (.run s r)) :=
  .equiv (.ref (.run_comm r s))

end Flick

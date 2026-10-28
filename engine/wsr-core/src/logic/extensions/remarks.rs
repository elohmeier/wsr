//! WSRDLL.DLL RemarksData (0x4015e6): a without-replacement remark pool and its independent RNG.
use crate::pb::real::cast;

use crate::{
    content::{ContentPack, TextId},
    logic::newgame::LocalTime,
    pb::{pbstr::PbStr, rng::Rng},
};
/// DLL-private state persists across games in the same application process.
#[derive(Default)]
pub struct Remarks {
    available: u32,
}
impl Remarks {
    pub fn next(&mut self, now: &LocalTime, content: &ContentPack) -> PbStr {
        let mut rng = Rng::default();
        rng.randomize_timer(now.timer_ms());
        for _ in 0..300 {
            let k = cast::<usize, _>(rng.rnd_range(1, 27) - 1);
            if self.available & (1 << k) != 0 {
                self.available &= !(1 << k);
                return content.text(TextId::bankruptcy_remark(k).unwrap()).clone();
            }
        }
        self.available = (1 << 27) - 1;
        let k = cast::<usize, _>(rng.rnd_range(1, 27) - 1);
        self.available &= !(1 << k);
        content.text(TextId::bankruptcy_remark(k).unwrap()).clone()
    }
}

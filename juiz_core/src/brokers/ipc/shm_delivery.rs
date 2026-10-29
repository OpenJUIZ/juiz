//! IPC ブローカー (受け手) の共有メモリの扱い (docs/spec/large-data.md §6.4〜§6.6)
//!
//! - create / shm / attach: 送り手の領域を読み取り専用で対応付け、接続が閉じるまで持つ
//! - 本体が共有メモリにある Topic の push: 本体を読まずにすぐ応答し、通知を Topic ごとの
//!   「最新の 1 件だけを持つ受け箱」に入れる。前の通知が残っていたら捨てて数える (dropped)
//! - Topic ごとの配達のスレッドが受け箱から取り出し、共有メモリから読んで、今と同じく Topic に push する。
//!   読む前か読んでいる最中に上書きされていたら捨てて数える (overwritten)

use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use juiz_sdk::anyhow::anyhow;
use juiz_sdk::image::DynamicImage;

use super::shm::{RingLayout, ShmReadError, ShmReader};
use crate::brokers::core_broker::CoreBrokerPtr;
use crate::brokers::frame::{capsule_from_body_bytes_reusing, BodyDesc, IncomingBody, IncomingFrame, ShmRef};
use crate::prelude::*;
use crate::topics::SharedMemoryStats;

/// 接続ごとの、対応付けた領域
pub type Regions = HashMap<String, Arc<ShmReader>>;

/// 受け箱に入れる通知
struct Notice {
    reader: Arc<ShmReader>,
    shm: ShmRef,
    desc: BodyDesc,
    arg_name: String,
    param: HashMap<String, String>,
}

struct Mailbox {
    latest: Mutex<Option<Notice>>,
    cv: Condvar,
}

/// 本体を共有メモリから読んで Topic に push する関数 (IpcBroker の要求の処理と同じ道を通す)
pub type Deliver = Arc<dyn Fn(CapsuleMap) -> JuizResult<CapsulePtr> + Send + Sync>;

/// Topic ごとの受け箱と配達のスレッド
pub struct ShmDelivery {
    own_name: String,
    /// CoreBrokerPtr は Sync でないので Mutex に入れる
    core_broker: Mutex<CoreBrokerPtr>,
    deliver: Deliver,
    /// 配達のスレッドを止める印。stop のたびに新しいものに替える
    stopping: Mutex<Arc<AtomicBool>>,
    mailboxes: Mutex<HashMap<String, Arc<Mailbox>>>,
}

fn param_of<'a>(param: &'a HashMap<String, String>, key: &str) -> JuizResult<&'a str> {
    param
        .get(key)
        .map(|s| s.as_str())
        .ok_or_else(|| anyhow!("'{key}' is required in the shared memory request"))
}

/// 要求が class_name / function_name のものか
pub fn is_request(frame: &IncomingFrame, class_name: &str, function_name: &str) -> bool {
    frame.header.param.as_ref().is_some_and(|p| {
        p.get("class_name").map(|s| s.as_str()) == Some(class_name)
            && p.get("function_name").map(|s| s.as_str()) == Some(function_name)
    })
}

impl ShmDelivery {
    pub fn new(own_name: String, core_broker: CoreBrokerPtr, deliver: Deliver) -> Self {
        ShmDelivery {
            own_name,
            core_broker: Mutex::new(core_broker),
            deliver,
            stopping: Mutex::new(Arc::new(AtomicBool::new(false))),
            mailboxes: Mutex::new(HashMap::new()),
        }
    }

    /// 受け手の Topic の数え上げ。Topic がまだ無ければ None
    fn stats_of(&self, topic_name: &str) -> Option<Arc<SharedMemoryStats>> {
        let core_broker = self.core_broker.lock().ok()?.clone();
        stats_of(&core_broker, topic_name)
    }

    /// create / shm / attach (§6.6)。Unix は届いた記述子を、Windows は名前で領域を開く
    pub fn attach(
        &self,
        frame: &IncomingFrame,
        regions: &mut Regions,
        #[cfg(unix)] fd: Option<super::shm::OwnedFd>,
    ) -> JuizResult<()> {
        let param = frame
            .header
            .param
            .as_ref()
            .ok_or_else(|| anyhow!("attach request has no 'param'"))?;
        let region = param_of(param, "region")?.to_owned();
        let num = |key: &str| -> JuizResult<u64> {
            param_of(param, key)?
                .parse::<u64>()
                .map_err(|e| anyhow!("'{key}' of the attach request is not a number ({e})"))
        };
        let slots: u32 = num("slots")?
            .try_into()
            .map_err(|_| anyhow!("'slots' is too large"))?;
        let layout = RingLayout::new(slots, num("slot_bytes")?)?;
        if layout.data_offset != num("data_offset")? {
            return Err(anyhow!("'data_offset' of the attach request does not match the layout"));
        }
        #[cfg(unix)]
        let reader = {
            let fd = fd.ok_or_else(|| anyhow!("attach request came without a file descriptor"))?;
            ShmReader::open(fd, &region, layout)?
        };
        #[cfg(windows)]
        let reader = ShmReader::open(&region, layout)?;
        log::debug!(
            "ipc://{}: shared memory {region} is attached ({} slots x {} bytes)",
            self.own_name,
            layout.slots,
            layout.slot_bytes
        );
        if let Some(topic_name) = param.get("topic_name") {
            if let Some(stats) = self.stats_of(topic_name) {
                stats.set_layout(layout.slots, layout.slot_bytes);
            }
        }
        regions.insert(region, Arc::new(reader));
        Ok(())
    }

    /// 本体が共有メモリにある Topic の push を受け箱に入れる (§6.5)。本体は読まない
    pub fn enqueue(&self, frame: IncomingFrame, regions: &Regions) -> JuizResult<()> {
        if !is_request(&frame, "topic", "push") {
            return Err(anyhow!(
                "shared memory can be used only for topic push (docs/spec/large-data.md §6.1)"
            ));
        }
        let mut frame = frame;
        let param = frame.header.param.take().unwrap_or_default();
        let topic_name = param_of(&param, "topic_name")?.to_owned();
        let map = frame.header.map.take().unwrap_or_default();
        if map.len() != 1 {
            return Err(anyhow!("a topic push in shared memory must have one argument"));
        }
        let (arg_name, repr) = map.into_iter().next().unwrap();
        let index = repr
            .get("body")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| anyhow!("the argument of a topic push in shared memory must be a body"))?;
        let Some(IncomingBody::Shared(desc)) = frame.take_body(index as usize) else {
            return Err(anyhow!("body {index} is not in shared memory"));
        };
        let shm = desc
            .shm()
            .cloned()
            .ok_or_else(|| anyhow!("body {index} has no shared memory reference"))?;
        let reader = regions
            .get(&shm.region)
            .cloned()
            .ok_or_else(|| anyhow!("unknown shared memory region {}", shm.region))?;
        let notice = Notice {
            reader,
            shm,
            desc,
            arg_name,
            param,
        };
        let mailbox = self.mailbox(&topic_name)?;
        let replaced = {
            let mut latest = mailbox
                .latest
                .lock()
                .map_err(|_| anyhow!("mailbox lock is poisoned"))?;
            latest.replace(notice).is_some()
        };
        mailbox.cv.notify_one();
        match self.stats_of(&topic_name) {
            Some(stats) => {
                stats.set_active();
                if replaced {
                    SharedMemoryStats::add(&stats.dropped);
                }
            }
            None => log::warn!("ipc://{}: topic {topic_name} is not found", self.own_name),
        }
        Ok(())
    }

    /// Topic の受け箱。無ければ作り、配達のスレッドを始める
    fn mailbox(&self, topic_name: &str) -> JuizResult<Arc<Mailbox>> {
        let mut mailboxes = self
            .mailboxes
            .lock()
            .map_err(|_| anyhow!("mailboxes lock is poisoned"))?;
        if let Some(m) = mailboxes.get(topic_name) {
            return Ok(m.clone());
        }
        let mailbox = Arc::new(Mailbox {
            latest: Mutex::new(None),
            cv: Condvar::new(),
        });
        let (m, deliver, stopping, own_name, topic, core_broker) = (
            mailbox.clone(),
            self.deliver.clone(),
            self.stopping
                .lock()
                .map_err(|_| anyhow!("stopping lock is poisoned"))?
                .clone(),
            self.own_name.clone(),
            topic_name.to_owned(),
            self.core_broker
                .lock()
                .map_err(|_| anyhow!("core broker lock is poisoned"))?
                .clone(),
        );
        std::thread::Builder::new()
            .name(format!("ipc-shm-{own_name}"))
            .spawn(move || deliver_loop(m, deliver, stopping, own_name, topic, core_broker))?;
        mailboxes.insert(topic_name.to_owned(), mailbox.clone());
        Ok(mailbox)
    }

    /// 配達のスレッドを止める。受け箱に残った通知は捨てる。
    /// スレッドは待たない (止めるのが CoreBroker のロックの中のこともあり、配達中のスレッドがそのロックを待つため)
    pub fn stop(&self) {
        if let Ok(mut s) = self.stopping.lock() {
            s.store(true, Ordering::SeqCst);
            // 次に start したときのために、新しい印に替える (古いスレッドは古い印を見て終わる)
            *s = Arc::new(AtomicBool::new(false));
        }
        if let Ok(mut mailboxes) = self.mailboxes.lock() {
            for m in mailboxes.values() {
                // 待っているスレッドが必ず起きるように、ロックを取ってから知らせる
                if let Ok(mut latest) = m.latest.lock() {
                    latest.take();
                }
                m.cv.notify_all();
            }
            mailboxes.clear();
        }
    }
}

fn stats_of(core_broker: &CoreBrokerPtr, topic_name: &str) -> Option<Arc<SharedMemoryStats>> {
    let cb = core_broker.lock().ok()?;
    let topic = cb.worker().store().topics.get(topic_name)?;
    Some(topic.shared_memory_stats())
}

/// 使い回しを待つ、届けた画像の数。Topic と subscribe した Process の引数は最後の 1 件を持ち続けるので、
/// 使い終わるのは 1 つ前より古いもの。それより多くは持たない (使われていない領域を抱え続けないため)
const KEEP_DELIVERED: usize = 2;

/// 届けた画像のうち、ほかに参照の無いものを 1 つ取り出す。参照のあるものは残す
fn take_unused_image(delivered: &mut VecDeque<CapsulePtr>) -> Option<DynamicImage> {
    for i in 0..delivered.len() {
        let p = delivered.remove(i)?;
        match p.try_into_capsule() {
            Ok(c) => return c.to_image(),
            Err(p) => delivered.insert(i, p),
        }
    }
    None
}

fn deliver_loop(
    mailbox: Arc<Mailbox>,
    deliver: Deliver,
    stopping: Arc<AtomicBool>,
    own_name: String,
    topic_name: String,
    core_broker: CoreBrokerPtr,
) {
    // 届けた画像 (古い順)。次に受け取るときに、誰も持っていないものがあればその領域を使い回す
    let mut delivered: VecDeque<CapsulePtr> = VecDeque::new();
    loop {
        let notice = {
            let Ok(mut latest) = mailbox.latest.lock() else {
                return;
            };
            loop {
                if stopping.load(Ordering::SeqCst) {
                    return;
                }
                if let Some(n) = latest.take() {
                    break n;
                }
                latest = match mailbox.cv.wait_timeout(latest, Duration::from_millis(500)) {
                    Ok((l, _)) => l,
                    Err(_) => return,
                };
            }
        };
        // 前に届けた画像のうち、もう誰も持っていないものの領域に写す (large-data.md §6.9 の「受け手の領域の使い回し」)
        let mut spare = take_unused_image(&mut delivered);
        let had_spare = spare.is_some();
        let desc = notice.desc.clone();
        let faults_before = minor_page_faults();
        let copy_start = Instant::now();
        let read = notice
            .reader
            .read(&notice.shm, |bytes| capsule_from_body_bytes_reusing(&desc, bytes, &mut spare));
        let copy_time = copy_start.elapsed();
        let faults = minor_page_faults().saturating_sub(faults_before);
        match read {
            Ok(capsule) => {
                let capsule: CapsulePtr = capsule.into();
                let mut cm = CapsuleMap::new();
                for (k, v) in notice.param.iter() {
                    cm.set_param(k, v);
                }
                cm.insert(notice.arg_name.clone(), capsule.clone());
                let push_start = Instant::now();
                if let Err(e) = deliver(cm) {
                    log::error!("ipc://{own_name}: topic push of {topic_name} (shared memory) failed. {e:#}");
                }
                let push_time = push_start.elapsed();
                if let Some(stats) = stats_of(&core_broker, &topic_name) {
                    SharedMemoryStats::add(&stats.received);
                    if had_spare && spare.is_none() {
                        SharedMemoryStats::add(&stats.reused);
                    }
                    SharedMemoryStats::add_n(&stats.receive_copy_ns, copy_time.as_nanos() as u64);
                    SharedMemoryStats::add_n(&stats.receive_page_faults, faults);
                    SharedMemoryStats::add_n(&stats.receive_push_ns, push_time.as_nanos() as u64);
                }
                delivered.push_back(capsule);
                if delivered.len() > KEEP_DELIVERED {
                    delivered.pop_front();
                }
            }
            Err(ShmReadError::Overwritten) => {
                log::debug!("ipc://{own_name}: a frame of {topic_name} was overwritten before it was read");
                if let Some(stats) = stats_of(&core_broker, &topic_name) {
                    SharedMemoryStats::add(&stats.overwritten);
                }
            }
            Err(ShmReadError::Invalid(e)) => {
                log::error!("ipc://{own_name}: can not read {topic_name} from shared memory. {e:#}");
            }
        }
    }
}

/// このスレッドでこれまでに起きたページフォールト (minor) の数。Linux 以外は 0
fn minor_page_faults() -> u64 {
    #[cfg(target_os = "linux")]
    {
        let mut usage = std::mem::MaybeUninit::<libc::rusage>::zeroed();
        // SAFETY: usage は書き込み先として十分な大きさがある
        if unsafe { libc::getrusage(libc::RUSAGE_THREAD, usage.as_mut_ptr()) } == 0 {
            // SAFETY: getrusage が成功したので初期化されている
            return unsafe { usage.assume_init() }.ru_minflt as u64;
        }
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn image_ptr(w: u32) -> CapsulePtr {
        Capsule::from(DynamicImage::new_rgb8(w, 1)).into()
    }

    #[test]
    fn only_images_nobody_holds_are_taken() {
        let (old, held) = (image_ptr(1), image_ptr(2));
        let holder = held.clone();
        let mut delivered: VecDeque<CapsulePtr> = [held, old].into_iter().collect();
        // 古い順に見て、参照の残っているもの (幅 2) は飛ばし、誰も持っていないもの (幅 1) を取り出す
        assert_eq!(take_unused_image(&mut delivered).map(|i| i.width()), Some(1));
        assert_eq!(delivered.len(), 1);
        assert_eq!(take_unused_image(&mut delivered), None);
        assert_eq!(delivered.len(), 1);
        drop(holder);
        assert_eq!(take_unused_image(&mut delivered).map(|i| i.width()), Some(2));
        assert!(delivered.is_empty());
    }
}

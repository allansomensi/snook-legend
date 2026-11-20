//! Coin packs and months of VIP bought in the Microsoft Store (the Store
//! build on Windows).
//!
//! The game only drives the Store's own pieces: it asks for the add-ons'
//! prices, opens the Store's purchase dialog, and turns the server's ticket
//! into a Microsoft Store ID key. It never decides that something was
//! bought: the server asks Microsoft, consumes the purchase and credits the
//! coins, and the month of VIP (see `crates/server/src/msstore.rs`). A
//! purchase that did not arrive (the game closed, the network dropped) is
//! found again by the next check, which runs once per session when the
//! shop's offers come in.
//!
//! The Store's calls wait on the network and its dialog, so they run on a
//! thread of their own; answers come back through [`StoreApi::poll`].

use std::sync::mpsc::{Receiver, Sender, channel};

/// What the game asks of the Store.
#[derive(Debug, Clone)]
// Read on Windows only (elsewhere the Store is never reached).
#[cfg_attr(not(all(windows, feature = "store")), allow(dead_code))]
pub enum Request {
    /// The prices of these add-ons (Store IDs), in the player's currency.
    Prices(Vec<String>),
    /// Opens the Store's purchase dialog for this add-on.
    Buy(String),
    /// Makes a Microsoft Store ID key from the server's ticket.
    Key { ticket: String, user_id: String },
}

/// What the Store answered.
#[derive(Debug, Clone, PartialEq)]
pub enum Reply {
    /// (Store ID, price as the Store shows it) for each add-on it knows.
    Prices(Vec<(String, String)>),
    Bought(Purchase),
    Key(Result<String, String>),
}

#[derive(Debug, Clone, PartialEq)]
// Read on Windows only (elsewhere the Store is never reached).
#[cfg_attr(not(all(windows, feature = "store")), allow(dead_code))]
pub enum Purchase {
    /// Paid (or paid before and not received yet): the server must check.
    Paid,
    /// The player closed the dialog without buying.
    Cancelled,
    /// The Store could not complete it (no charge).
    Failed(String),
}

/// The Store's purchase API on a thread of its own.
pub struct StoreApi {
    tx: Sender<Request>,
    rx: Receiver<Reply>,
}

impl StoreApi {
    pub fn start() -> StoreApi {
        let (tx, requests) = channel::<Request>();
        let (answers, rx) = channel::<Reply>();
        let _ = std::thread::Builder::new()
            .name("store".into())
            .spawn(move || worker(requests, answers));
        StoreApi { tx, rx }
    }

    pub fn send(&self, request: Request) {
        let _ = self.tx.send(request);
    }

    pub fn poll(&self) -> Option<Reply> {
        self.rx.try_recv().ok()
    }
}

#[cfg(all(windows, feature = "store"))]
fn worker(requests: Receiver<Request>, answers: Sender<Reply>) {
    // A multithreaded apartment for this thread (WinRT objects are agile).
    // SAFETY: called once, on this thread, before any WinRT call.
    unsafe {
        let _ = windows::Win32::System::WinRT::RoInitialize(
            windows::Win32::System::WinRT::RO_INIT_MULTITHREADED,
        );
    }
    for request in requests {
        let reply = match request {
            Request::Prices(ids) => Reply::Prices(store::prices(&ids).unwrap_or_else(|e| {
                eprintln!("store: prices: {e}");
                Vec::new()
            })),
            Request::Buy(id) => Reply::Bought(store::buy(&id)),
            Request::Key { ticket, user_id } => {
                Reply::Key(store::key(&ticket, &user_id).map_err(|e| e.to_string()))
            }
        };
        if answers.send(reply).is_err() {
            return;
        }
    }
}

/// Anywhere else there is no Store: nothing is on sale. (Development builds
/// can show made-up prices for screenshots: `SNOOK_LEGEND_STORE_PRICES`, one
/// per pack separated by `;`. Nothing can be bought with them.)
#[cfg(not(all(windows, feature = "store")))]
fn worker(requests: Receiver<Request>, answers: Sender<Reply>) {
    let shown = crate::dev::var("SNOOK_LEGEND_STORE_PRICES").unwrap_or_default();
    for request in requests {
        let reply = match request {
            Request::Prices(ids) => Reply::Prices(
                ids.into_iter()
                    .zip(shown.split(';').map(str::trim).filter(|p| !p.is_empty()))
                    .map(|(id, price)| (id, price.to_string()))
                    .collect(),
            ),
            Request::Buy(_) => Reply::Bought(Purchase::Failed("no Microsoft Store here".into())),
            Request::Key { .. } => Reply::Key(Err("no Microsoft Store here".into())),
        };
        if answers.send(reply).is_err() {
            return;
        }
    }
}

#[cfg(all(windows, feature = "store"))]
mod store {
    use windows::Services::Store::{StoreContext, StorePurchaseStatus};
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::Shell::IInitializeWithWindow;
    use windows::core::{HSTRING, Interface, Result};
    use windows_collections::IIterable;

    use super::Purchase;

    /// The add-on kinds asked for (the Store IDs narrow it to ours).
    const KINDS: [&str; 3] = ["UnmanagedConsumable", "Consumable", "Durable"];

    /// The Store's context, with the game's window as the owner of its
    /// dialogs (a desktop app must say which window that is).
    fn context() -> Result<StoreContext> {
        let context = StoreContext::GetDefault()?;
        if let Some(hwnd) = super::game_window() {
            let init: IInitializeWithWindow = context.cast()?;
            // SAFETY: a window handle of this process, alive while the game runs.
            unsafe { init.Initialize(HWND(hwnd))? };
        }
        Ok(context)
    }

    fn strings(list: &[&str]) -> IIterable<HSTRING> {
        IIterable::from(list.iter().map(|s| HSTRING::from(*s)).collect::<Vec<_>>())
    }

    pub fn prices(ids: &[String]) -> Result<Vec<(String, String)>> {
        let context = context()?;
        let ids: Vec<&str> = ids.iter().map(String::as_str).collect();
        let result = context.GetStoreProductsAsync(&strings(&KINDS), &strings(&ids))?.join()?;
        result.ExtendedError()?.ok()?;
        let products = result.Products()?;
        let mut out = Vec::new();
        for id in ids {
            // An add-on the Store doesn't know (not published yet, or not in
            // this market) is simply not offered.
            let Ok(product) = products.Lookup(&HSTRING::from(id)) else { continue };
            let price = product.Price()?.FormattedPrice()?.to_string();
            if !price.is_empty() {
                out.push((id.to_string(), price));
            }
        }
        Ok(out)
    }

    pub fn buy(id: &str) -> Purchase {
        let result = context().and_then(|c| c.RequestPurchaseAsync(&HSTRING::from(id))?.join());
        let result = match result {
            Ok(r) => r,
            Err(e) => return Purchase::Failed(e.message()),
        };
        match result.Status() {
            // Bought before and not received: the server's check gets it.
            Ok(StorePurchaseStatus::Succeeded) | Ok(StorePurchaseStatus::AlreadyPurchased) => {
                Purchase::Paid
            }
            Ok(StorePurchaseStatus::NotPurchased) => Purchase::Cancelled,
            Ok(other) => {
                let detail = result.ExtendedError().map(|e| e.message()).unwrap_or_default();
                Purchase::Failed(format!("status {}: {detail}", other.0))
            }
            Err(e) => Purchase::Failed(e.message()),
        }
    }

    pub fn key(ticket: &str, user_id: &str) -> Result<String> {
        let context = context()?;
        let key = context
            .GetCustomerCollectionsIdAsync(&HSTRING::from(ticket), &HSTRING::from(user_id))?
            .join()?;
        Ok(key.to_string())
    }
}

/// What the shop does next (applied by `Online`).
#[derive(Debug, Clone, PartialEq)]
pub enum Effect {
    Send(snook_legend_proto::ClientMsg),
    /// A toast: its text (already translated) and whether it is good news.
    Toast(String, bool),
}

/// What the Store part of the shop is doing (shown on its buttons).
#[derive(Debug, Clone, PartialEq)]
pub enum Busy {
    /// The Store's purchase dialog is open for this add-on.
    Buying(String),
    /// The server is checking the purchases; `loud` after a purchase (the
    /// player waits for an answer), quiet for the check of each session.
    Checking { loud: bool },
}

/// How long to wait before looking again for a purchase the Store said was
/// made but the collection doesn't show yet (it can take a moment).
const RECHECKS: [f32; 3] = [3.0, 10.0, 30.0];

/// The Microsoft Store build's coin packs and month of VIP: prices, the
/// purchase and the server's check that follows it.
#[derive(Default)]
pub struct StoreShop {
    api: Option<StoreApi>,
    /// Store ID -> price, once the Store answered (`None` while waiting).
    pub prices: Option<std::collections::HashMap<String, String>>,
    asked: Vec<String>,
    /// The Store ID of the month of VIP (empty: not on sale).
    vip_id: String,
    /// The purchase being confirmed is a month of VIP (what the notices
    /// talk about).
    vip: bool,
    pub busy: Option<Busy>,
    /// The quiet check of this session was made.
    checked: bool,
    /// Looks left, and the time to the next one.
    rechecks_left: usize,
    recheck_in: Option<f32>,
    /// Seconds the current check has waited for the server.
    check_age: f32,
}

/// A check without an answer for this long (the connection dropped) is given
/// up: the next visit to the shop checks again, so nothing paid is lost.
const CHECK_TIMEOUT: f32 = 45.0;

impl StoreShop {
    fn api(&mut self) -> &StoreApi {
        self.api.get_or_insert_with(StoreApi::start)
    }

    /// The server's offers came in (the coin packs' Store IDs and the month
    /// of VIP's, empty when not on sale): ask the Store for the prices, and
    /// receive what an earlier session paid for but never got.
    pub fn offers(&mut self, packs: Vec<String>, vip_id: String, signed_in: bool) -> Vec<Effect> {
        let mut store_ids = packs;
        if !vip_id.is_empty() {
            store_ids.push(vip_id.clone());
        }
        self.vip_id = vip_id;
        if !crate::channel::STORE || store_ids.is_empty() {
            return Vec::new();
        }
        if store_ids != self.asked {
            self.asked = store_ids.clone();
            self.prices = None;
            self.api().send(Request::Prices(store_ids));
        }
        if signed_in && !self.checked && self.busy.is_none() {
            self.checked = true;
            return self.check(false);
        }
        Vec::new()
    }

    /// Opens the Store's purchase dialog for `store_id`.
    pub fn buy(&mut self, store_id: &str) {
        if self.busy.is_none() {
            self.vip = !self.vip_id.is_empty() && store_id == self.vip_id;
            self.busy = Some(Busy::Buying(store_id.to_string()));
            self.recheck_in = None;
            self.api().send(Request::Buy(store_id.to_string()));
        }
    }

    fn check(&mut self, loud: bool) -> Vec<Effect> {
        self.busy = Some(Busy::Checking { loud });
        self.check_age = 0.0;
        vec![Effect::Send(snook_legend_proto::ClientMsg::StoreTicket)]
    }

    /// The Store's answers, and the looks that are due.
    pub fn update(&mut self, dt: f32) -> Vec<Effect> {
        let mut out = Vec::new();
        while let Some(reply) = self.api.as_ref().and_then(StoreApi::poll) {
            out.extend(self.reply(reply));
        }
        if matches!(self.busy, Some(Busy::Checking { .. })) {
            self.check_age += dt;
            if self.check_age > CHECK_TIMEOUT {
                let loud = self.take_busy();
                self.checked = false;
                self.recheck_in = None;
                if loud {
                    out.push(Effect::Toast(self.not_confirmed(), false));
                }
            }
        }
        if let Some(wait) = &mut self.recheck_in {
            *wait -= dt;
            if *wait <= 0.0 && self.busy.is_none() {
                self.recheck_in = None;
                out.extend(self.check(true));
            }
        }
        out
    }

    fn reply(&mut self, reply: Reply) -> Vec<Effect> {
        use crate::i18n::t;
        match reply {
            Reply::Prices(list) => {
                self.prices = Some(list.into_iter().collect());
                Vec::new()
            }
            Reply::Bought(Purchase::Paid) => {
                self.rechecks_left = RECHECKS.len();
                self.check(true)
            }
            Reply::Bought(Purchase::Cancelled) => {
                self.busy = None;
                Vec::new()
            }
            Reply::Bought(Purchase::Failed(why)) => {
                eprintln!("store: purchase: {why}");
                // Whatever happened, anything that was paid is found by a
                // check (quietly).
                let mut out = vec![Effect::Toast(
                    t("The Microsoft Store couldn't complete the purchase.").to_string(),
                    false,
                )];
                out.extend(self.check(false));
                out
            }
            // No one is signed in to the Store: there is nothing to check.
            Reply::Key(Ok(key)) if key.trim().is_empty() => {
                let loud = self.take_busy();
                self.checked = false;
                self.recheck_in = None;
                if loud {
                    let text = t(
                        "Sign in to the Microsoft Store with your Microsoft account, then open the shop again.",
                    );
                    vec![Effect::Toast(text.to_string(), false)]
                } else {
                    Vec::new()
                }
            }
            Reply::Key(Ok(key)) => {
                vec![Effect::Send(snook_legend_proto::ClientMsg::StoreRedeem { key })]
            }
            Reply::Key(Err(why)) => {
                eprintln!("store: key: {why}");
                let loud = self.take_busy();
                // Tried again when the shop opens next.
                self.checked = false;
                if loud { vec![Effect::Toast(self.not_confirmed(), false)] } else { Vec::new() }
            }
        }
    }

    /// The server's ticket: the Store makes the key from it.
    pub fn ticket(&mut self, ticket: String, user_id: String) {
        if matches!(self.busy, Some(Busy::Checking { .. })) {
            self.api().send(Request::Key { ticket, user_id });
        }
    }

    /// Ends the current step; whether the player was waiting for it.
    fn take_busy(&mut self) -> bool {
        matches!(self.busy.take(), Some(Busy::Checking { loud: true }))
    }

    /// The purchase could not be confirmed now (it arrives later).
    fn not_confirmed(&self) -> String {
        crate::i18n::t(if self.vip {
            "The Microsoft Store couldn't confirm the purchase right now. Your VIP month starts the next time you open the shop."
        } else {
            "The Microsoft Store couldn't confirm the purchase right now. Its coins arrive the next time you open the shop."
        })
        .to_string()
    }

    /// The server checked (or credited a purchase by itself). `vip_until`:
    /// a month of VIP was credited, and VIP lasts until then (0: none).
    pub fn redeemed(
        &mut self,
        coins: i64,
        pending: bool,
        error: &str,
        vip_until: i64,
    ) -> Vec<Effect> {
        use crate::i18n::{t, tw};
        // An answer to another step (the server's own retries) leaves
        // the purchase dialog alone.
        let loud = match self.busy {
            Some(Busy::Checking { .. }) => self.take_busy(),
            _ => false,
        };
        if coins > 0 || vip_until > 0 {
            self.recheck_in = None;
            let text = if vip_until > 0 {
                let until = crate::profile::date(vip_until);
                tw(
                    "You are VIP until {d}, and {n} coins were added to your wallet.",
                    &[("d", &until), ("n", &coins)],
                )
            } else {
                tw("{n} coins added to your wallet.", &[("n", &coins)])
            };
            return vec![Effect::Toast(text, true)];
        }
        if !error.is_empty() {
            self.checked = false;
            return if loud {
                vec![Effect::Toast(crate::i18n::server_text(error), false)]
            } else {
                Vec::new()
            };
        }
        if !loud {
            return Vec::new();
        }
        if pending {
            let text = t(if self.vip {
                "Microsoft is confirming your purchase: your VIP month starts in a moment."
            } else {
                "Microsoft is confirming your purchase: the coins arrive in a moment."
            });
            return vec![Effect::Toast(text.to_string(), true)];
        }
        // Paid, but not in the collection yet: look again a little later.
        if self.rechecks_left > 0 {
            let k = RECHECKS.len() - self.rechecks_left;
            self.rechecks_left -= 1;
            self.recheck_in = Some(RECHECKS[k]);
            return Vec::new();
        }
        self.checked = false;
        let text = t(if self.vip {
            "The purchase hasn't reached us yet. Your VIP month starts the next time you open the shop."
        } else {
            "The purchase hasn't reached us yet. Its coins arrive the next time you open the shop."
        });
        vec![Effect::Toast(text.to_string(), false)]
    }

    /// The player signed out: the next account makes its own check. (The
    /// Store's thread and the prices stay; a purchase dialog still open
    /// finishes, and whatever it paid is found by the next check.)
    pub fn signed_out(&mut self) {
        self.busy = None;
        self.checked = false;
        self.recheck_in = None;
        self.rechecks_left = 0;
    }

    /// Waiting for the Store or the server (the buttons say so).
    pub fn waiting(&self) -> bool {
        matches!(self.busy, Some(Busy::Buying(_)) | Some(Busy::Checking { loud: true }))
            || self.recheck_in.is_some()
    }
}

/// The game's window: the top-level window of this process with the game's
/// title.
#[cfg(all(windows, feature = "store"))]
fn game_window() -> Option<*mut core::ffi::c_void> {
    use core::ffi::c_void;

    #[link(name = "user32")]
    unsafe extern "system" {
        fn EnumWindows(
            callback: unsafe extern "system" fn(*mut c_void, isize) -> i32,
            data: isize,
        ) -> i32;
        fn GetWindowThreadProcessId(hwnd: *mut c_void, pid: *mut u32) -> u32;
        fn GetWindowTextW(hwnd: *mut c_void, text: *mut u16, max: i32) -> i32;
    }

    struct Search {
        pid: u32,
        title: Vec<u16>,
        found: *mut c_void,
    }

    unsafe extern "system" fn visit(hwnd: *mut c_void, data: isize) -> i32 {
        // SAFETY: `data` is the `Search` below, alive for the whole
        // EnumWindows call; the buffer is as long as we say.
        unsafe {
            let search = &mut *(data as *mut Search);
            let mut pid = 0u32;
            GetWindowThreadProcessId(hwnd, &mut pid);
            if pid != search.pid {
                return 1;
            }
            let mut buf = [0u16; 64];
            let n = GetWindowTextW(hwnd, buf.as_mut_ptr(), buf.len() as i32).max(0) as usize;
            if buf[..n] == search.title[..] {
                search.found = hwnd;
                return 0;
            }
            1
        }
    }

    let mut search = Search {
        pid: std::process::id(),
        title: snook_legend_proto::GAME_NAME.encode_utf16().collect(),
        found: std::ptr::null_mut(),
    };
    // SAFETY: the callback only touches `search`, which outlives the call.
    unsafe { EnumWindows(visit, &mut search as *mut Search as isize) };
    (!search.found.is_null()).then_some(search.found)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn toasts(effects: &[Effect]) -> usize {
        effects.iter().filter(|e| matches!(e, Effect::Toast(..))).count()
    }

    #[test]
    fn a_purchase_is_checked_and_looked_for_again() {
        let mut shop =
            StoreShop { busy: Some(Busy::Buying("9AAAAAAAAAAA".into())), ..Default::default() };
        let effects = shop.reply(Reply::Bought(Purchase::Paid));
        assert_eq!(effects, [Effect::Send(snook_legend_proto::ClientMsg::StoreTicket)]);
        assert_eq!(shop.busy, Some(Busy::Checking { loud: true }));
        // Not in the collection yet: looked for again, three times.
        for _ in 0..RECHECKS.len() {
            assert!(shop.redeemed(0, false, "", 0).is_empty());
            assert!(shop.waiting());
            let effects = shop.update(60.0);
            assert_eq!(effects, [Effect::Send(snook_legend_proto::ClientMsg::StoreTicket)]);
        }
        let effects = shop.redeemed(0, false, "", 0);
        assert_eq!(toasts(&effects), 1);
        assert!(!shop.waiting());
        assert!(!shop.checked, "tried again when the shop opens");
    }

    #[test]
    fn coins_end_the_wait() {
        let mut shop =
            StoreShop { busy: Some(Busy::Checking { loud: true }), ..Default::default() };
        let effects = shop.redeemed(1300, false, "", 0);
        assert!(matches!(&effects[..], [Effect::Toast(_, true)]));
        assert_eq!(shop.busy, None);
        // Coins credited later by the server's own retries also show.
        assert_eq!(toasts(&shop.redeemed(500, false, "", 0)), 1);
    }

    #[test]
    fn quiet_checks_stay_quiet() {
        let mut shop =
            StoreShop { busy: Some(Busy::Checking { loud: false }), ..Default::default() };
        assert!(shop.redeemed(0, false, "", 0).is_empty());
        shop.busy = Some(Busy::Checking { loud: false });
        assert!(
            shop.redeemed(0, false, "The Microsoft Store can't be reached right now.", 0)
                .is_empty()
        );
        shop.busy = Some(Busy::Checking { loud: false });
        assert!(shop.reply(Reply::Key(Err("no".into()))).is_empty());
        assert_eq!(shop.busy, None);
    }

    #[test]
    fn a_failed_or_cancelled_purchase_claims_nothing() {
        let mut shop = StoreShop { busy: Some(Busy::Buying("9A".into())), ..Default::default() };
        assert!(shop.reply(Reply::Bought(Purchase::Cancelled)).is_empty());
        assert_eq!(shop.busy, None);
        shop.busy = Some(Busy::Buying("9A".into()));
        let effects = shop.reply(Reply::Bought(Purchase::Failed("x".into())));
        // A toast, and a quiet check (the server asks Microsoft; the game
        // never says what was bought).
        assert_eq!(toasts(&effects), 1);
        assert_eq!(shop.busy, Some(Busy::Checking { loud: false }));
    }

    #[test]
    fn a_check_without_an_answer_is_given_up() {
        let mut shop = StoreShop { checked: true, ..Default::default() };
        let _ = shop.check(true);
        assert!(shop.update(CHECK_TIMEOUT - 1.0).is_empty());
        assert_eq!(toasts(&shop.update(2.0)), 1);
        assert_eq!(shop.busy, None);
        assert!(!shop.checked, "checked again when the shop opens");
    }

    #[test]
    fn an_empty_key_means_no_one_is_signed_in_to_the_store() {
        let mut shop =
            StoreShop { busy: Some(Busy::Checking { loud: true }), ..Default::default() };
        let effects = shop.reply(Reply::Key(Ok(String::new())));
        assert_eq!(toasts(&effects), 1);
        assert!(!effects.iter().any(|e| matches!(e, Effect::Send(_))), "nothing sent");
        assert_eq!(shop.busy, None);
    }

    #[test]
    fn signing_out_lets_the_next_account_check() {
        let mut shop = StoreShop {
            checked: true,
            busy: Some(Busy::Checking { loud: false }),
            ..Default::default()
        };
        shop.signed_out();
        assert_eq!((shop.checked, shop.busy.clone()), (false, None));
    }

    #[test]
    fn a_month_of_vip_is_told_as_vip() {
        let _lang = crate::i18n::lang_lock();
        crate::i18n::set_lang(crate::i18n::Lang::English);
        let mut shop = StoreShop::default();
        let _ = shop.offers(vec!["9A".into()], "9V".into(), false);
        shop.buy("9V");
        assert!(shop.vip);
        let _ = shop.reply(Reply::Bought(Purchase::Paid));
        let effects = shop.redeemed(0, true, "", 0);
        assert!(matches!(&effects[..], [Effect::Toast(text, true)] if text.contains("VIP")));
        // Credited: the toast says until when, and the coins.
        shop.busy = Some(Busy::Checking { loud: true });
        let effects = shop.redeemed(400, false, "", 1_800_000_000);
        assert!(
            matches!(&effects[..], [Effect::Toast(text, true)] if text.contains("VIP") && text.contains("400"))
        );
        // A coin pack bought next talks about coins again.
        shop.busy = None;
        shop.buy("9A");
        assert!(!shop.vip);
        let _ = shop.reply(Reply::Bought(Purchase::Paid));
        let effects = shop.redeemed(0, true, "", 0);
        assert!(matches!(&effects[..], [Effect::Toast(text, true)] if !text.contains("VIP")));
    }

    #[test]
    fn a_purchase_dialog_is_not_opened_twice() {
        let mut shop = StoreShop::default();
        shop.buy("9A");
        shop.buy("9B");
        assert_eq!(shop.busy, Some(Busy::Buying("9A".into())));
    }

    /// Without the Store, nothing is sold and nothing is claimed.
    #[cfg(not(all(windows, feature = "store")))]
    #[test]
    fn without_the_store_nothing_is_bought() {
        let api = StoreApi::start();
        api.send(Request::Prices(vec!["9AAAAAAAAAAA".into()]));
        api.send(Request::Buy("9AAAAAAAAAAA".into()));
        api.send(Request::Key { ticket: "t".into(), user_id: "u".into() });
        let mut replies = Vec::new();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while replies.len() < 3 && std::time::Instant::now() < deadline {
            match api.poll() {
                Some(r) => replies.push(r),
                None => std::thread::sleep(std::time::Duration::from_millis(5)),
            }
        }
        assert_eq!(replies[0], Reply::Prices(Vec::new()));
        assert!(matches!(replies[1], Reply::Bought(Purchase::Failed(_))));
        assert!(matches!(replies[2], Reply::Key(Err(_))));
    }
}

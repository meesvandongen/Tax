#[path = "kernel.rs"]
mod kernel;

static mut SIM: Option<kernel::Sim> = None;

#[no_mangle]
pub extern "C" fn init(n: u32, w: u32, seed: u32) {
    unsafe { SIM = Some(kernel::Sim::new(n as usize, w as usize, seed)) }
}

#[no_mangle]
pub extern "C" fn step(k: u32) {
    #[allow(static_mut_refs)]
    let s = unsafe { SIM.as_mut().unwrap() };
    for _ in 0..k {
        s.step();
    }
}

#[no_mangle]
pub extern "C" fn hash() -> u32 {
    #[allow(static_mut_refs)]
    unsafe { SIM.as_ref().unwrap().hash() }
}

#[no_mangle]
pub extern "C" fn tax_total() -> f64 {
    #[allow(static_mut_refs)]
    unsafe { SIM.as_ref().unwrap().tax_total }
}

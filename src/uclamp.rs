use log::{info, warn};

#[derive(Default)]
#[repr(C)]
struct SchedAttr {
    size: u32,
    sched_policy: u32,
    sched_flags: u64,
    sched_nice: i32,
    sched_priority: u32,
    sched_runtime: u64,
    sched_deadline: u64,
    sched_period: u64,
    sched_util_min: u32,
    sched_util_max: u32,
}

pub fn set_uclamp(uclamp_min: u32, uclamp_max: u32) {
    let mut attr: SchedAttr = Default::default();
    let pid = unsafe { libc::getpid() };

    if unsafe {
        libc::syscall(
            libc::SYS_sched_getattr,
            pid,
            &mut attr,
            core::mem::size_of::<SchedAttr>(),
            0,
        )
    } != 0
    {
        warn!("Failed to set uclamp");
        return;
    }

    /* SCHED_FLAG_KEEP_POLICY |
     * SCHED_FLAG_KEEP_PARAMS |
     * SCHED_FLAG_UTIL_CLAMP_MIN |
     * SCHED_FLAG_UTIL_CLAMP_MAX */
    attr.sched_flags = 0x8 | 0x10 | 0x20 | 0x40;
    attr.sched_util_min = uclamp_min;
    attr.sched_util_max = uclamp_max;

    if unsafe { libc::syscall(libc::SYS_sched_setattr, pid, &mut attr, 0) } != 0 {
        warn!("Failed to set uclamp");
        return;
    }

    info!("Set task uclamp to {}:{}", uclamp_min, uclamp_max);
}

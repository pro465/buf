use std::panic;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::thread::{self, ScopedJoinHandle};

pub type Res<E> = Result<(), E>;

pub enum Error<U, D> {
    FromUpdate(U),
    FromDraw(D),
}

pub fn buffer<U, D, B, R1, R2>(bufs: &mut [Mutex<B>], mut update: U, mut draw: D) -> Error<R1, R2>
where
    U: FnMut(&mut B) -> Res<R1>,
    D: FnMut(&mut B) -> Res<R2> + Send,
    R2: Send,
    B: Send,
{
    let len = bufs.len();
    let stop = AtomicBool::new(false);
    let flag = AtomicBool::new(false);
    let ret = Mutex::new(None::<R2>);

    thread::scope(|s| {
        let handle = s.spawn(|| {
            while !flag.load(Ordering::Relaxed) {
                thread::park();
            }

            let mut di = 0;

            loop {
                if stop.load(Ordering::Relaxed) {
                    return;
                }

                if let Err(x) = (draw)(&mut *bufs[di].lock().unwrap()) {
                    *ret.lock().unwrap() = Some(x);
                    return;
                }

                di = (di + 1) % len;
            }
        });

        let mut ui = 0;

        loop {
            if let Some(x) = ret.lock().unwrap().take() {
                return Error::FromDraw(x);
            }

            let res = panic::catch_unwind(panic::AssertUnwindSafe(|| {
                (update)(&mut *bufs[ui].lock().unwrap())
            }));

            match res {
                Err(e) => {
                    join(handle, &stop);
                    panic::resume_unwind(e);
                }
                Ok(Err(x)) => {
                    join(handle, &stop);
                    return Error::FromUpdate(x);
                }
                _ => {}
            }

            ui = (ui + 1) % len;
            if !flag.swap(true, Ordering::Relaxed) {
                handle.thread().unpark();
            }
        }
    })
}

fn join(h: ScopedJoinHandle<()>, stop: &AtomicBool) {
    stop.store(true, Ordering::Relaxed);

    #[allow(unused_must_use)]
    {
        h.join();
    }
}

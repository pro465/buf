use std::panic;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Mutex;
use std::thread::{self, JoinHandle};

struct Wrapper<T>(*mut T);

unsafe impl<T> Send for Wrapper<T> {}

pub type Res<E> = Result<(), E>;

pub enum Error<U, D> {
    FromUpdate(U),
    FromDraw(D),
}

pub fn buffer<U, D, B, R1, R2>(bufs: &mut [B], mut update: U, mut draw: D) -> Error<R1, R2>
where
    U: FnMut(&mut B) -> Res<R1>,
    D: FnMut(&B) -> Res<R2> + Send,
    R2: Send,
{
    let len = bufs.len();
    let bufs = bufs.as_mut_ptr();
    let wrapper = Wrapper(bufs);

    let ui = &AtomicUsize::new(0);
    let di = &AtomicUsize::new(0);
    let stop = &AtomicBool::new(false);
    let ret = &Mutex::new(None::<R2>);

    let handle = {
        let closure = move || {
            // SAFETY: we are only accessing different locations from different threads at any
            // time, so this, in fact is safe to be Send
            let bufs = wrapper.0;

            loop {
                let di_val = di.load(Ordering::SeqCst);

                while di_val == ui.load(Ordering::SeqCst) {
                    std::hint::spin_loop();
                }

                if stop.load(Ordering::SeqCst) {
                    return;
                }

                // SAFETY: we already waited until the main thread was done, so the only way this
                // could be acceessed simultanously is if the main thread tried to access it while
                // we are `draw`ing, which is ruled out by the other thread checking to see if the
                // next item its gonna modify is being accessed by us and waiting until we are done
                if let Err(x) = (draw)(unsafe { &*bufs.add(di_val) }) {
                    *ret.lock().unwrap() = Some(x);
                    return;
                }

                di.store(di_val + 1 % len, Ordering::SeqCst);
            }
        };

        let f: Box<dyn FnOnce() + Send> = Box::new(closure);
        // SAFETY: we will close this thread before returning from this function's scope
        let f: Box<dyn FnOnce() + Send + 'static> = unsafe { std::mem::transmute(f) };

        thread::spawn(move || f())
    };

    loop {
        let ui_val = ui.load(Ordering::SeqCst);

        if let Some(x) = ret.lock().unwrap().take() {
            return Error::FromDraw(x);
        }

        let res = panic::catch_unwind(panic::AssertUnwindSafe(|| {
            // SAFETY: we are the only ones accessing this because if the other thread is trying
            // to access the same buf as us, it will wait until we are finished
            (update)(unsafe { &mut *bufs.add(ui_val) })
        }));

        match res {
            Err(e) => {
                join(handle, stop);
                panic::resume_unwind(e);
            }
            Ok(Err(x)) => {
                join(handle, stop);
                return Error::FromUpdate(x);
            }
            _ => {}
        }

        while ui_val + 1 % len == di.load(Ordering::SeqCst) {
            std::hint::spin_loop();
        }

        ui.store(ui_val + 1 % len, Ordering::SeqCst);
    }
}

fn join(h: JoinHandle<()>, stop: &AtomicBool) {
    stop.store(true, Ordering::SeqCst);

    #[allow(unused_must_use)]
    {
        h.join();
    }
}

use std::any::Any;
use std::panic;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::thread::{self, JoinHandle};

struct Wrapper<T: Send>(*mut T);

unsafe impl<T: Send> Send for Wrapper<T> {}

pub fn buffer<U, D, B>(bufs: &mut [B], mut update: U, mut draw: D) -> !
where
    B: Send,
    U: FnMut(&mut B),
    D: FnMut(&B) + Send,
{
    let len = bufs.len();
    let bufs = bufs.as_mut_ptr();
    let wrapper = Wrapper(bufs);

    let ui = &AtomicUsize::new(0);
    let di = &AtomicUsize::new(0);
    let stop = &AtomicBool::new(false);

    let handle = {
        let closure = move || {
            let bufs = wrapper.0;

            loop {
                let di_val = di.load(Ordering::SeqCst);

                while di_val == ui.load(Ordering::SeqCst) {
                    std::hint::spin_loop();
                }

                if stop.load(Ordering::SeqCst) {
                    return;
                }

                (draw)(unsafe { &*bufs.add(di_val) });

                di.store(di_val + 1 % len, Ordering::SeqCst);
            }
        };

        let b: Box<dyn FnOnce() + Send> = Box::new(closure);
        let b: Box<dyn FnOnce() + Send + 'static> = unsafe { std::mem::transmute(b) };

        thread::spawn(move || b())
    };

    loop {
        let ui_val = ui.load(Ordering::SeqCst);

        let res = panic::catch_unwind(panic::AssertUnwindSafe(|| {
            (update)(unsafe { &mut *bufs.add(ui_val) })
        }));

        if let Err(e) = res {
            handle_panic(e, handle, stop);
        }

        while ui_val + 1 % len == di.load(Ordering::SeqCst) {
            std::hint::spin_loop();
        }

        ui.store(ui_val + 1 % len, Ordering::SeqCst);
    }
}

fn handle_panic(e: Box<dyn Any + Send + 'static>, h: JoinHandle<()>, stop: &AtomicBool) -> ! {
    stop.store(true, Ordering::SeqCst);

    #[allow(unused_must_use)]
    {
        h.join();
    }

    panic::resume_unwind(e);
}

use std::any::Any;
use std::panic;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use std::thread::{self, JoinHandle};

struct Wrapper<T: Send>(*mut T);

unsafe impl<T: Send> Send for Wrapper<T> {}

pub fn buffer<U, D, B>(bufs: &mut [B], mut update: U, draw: D) -> !
where
    B: Send,
    U: FnMut(&mut B),
    D: FnMut(&B) + Send,
{
    let len = bufs.len();
    let bufs = bufs.as_mut_ptr();
    let wrapper = Wrapper(bufs);

    let ui = Arc::new(AtomicUsize::new(0));
    let ui_clone = ui.clone();
    let di = Arc::new(AtomicUsize::new(0));
    let di_clone = di.clone();

    let 

    let handle = {
        let closure = move || {
            let bufs = wrapper.0;

            loop {
                let di_val = di.load(Ordering::SeqCst);

                if di_val == ui_clone.load(Ordering::SeqCst) {
                    continue;
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
            handle_panic(e, handle);
        }

        while ui_val + 1 % len == di_clone.load(Ordering::SeqCst) {
            std::hint::spin_loop();
        }

        ui.store(ui_val + 1 % len, Ordering::SeqCst);
    }
}

fn handle_panic(e: Box<dyn Any + Send + 'static>, h: JoinHandle<()>) -> ! {
    #[allow(unused_must_use)]
    {
        h.join();
    }
    panic::resume_unwind(e);
}

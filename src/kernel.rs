use super::kernel_types::{
    KernelExportID, KernelPromiseID, KernelResolverID, Target, VatName,
};
use super::vat::{Dispatch, Syscall, VatSyscall};
use super::vat_types::VatExportID;
use std::collections::{HashMap, VecDeque};

pub struct PendingDelivery {
    target: Target,
    method: String,
    args: u8,
    resolver: KernelResolverID,
}

//#[derive(Debug)]
pub struct Kernel {
    vats: HashMap<VatName, Box<dyn Dispatch>>,
    run_queue: VecDeque<PendingDelivery>,
    next_promise_resolver_id: u32,
}

impl Kernel {
    pub fn new(vats: HashMap<VatName, Box<dyn Dispatch>>) -> Self {
        Kernel {
            vats,
            run_queue: VecDeque::new(),
            next_promise_resolver_id: 0,
        }
    }

    pub fn _add_vat(&mut self, name: &VatName, dispatch: impl Dispatch + 'static) {
        let vn = VatName(name.0.clone());
        self.vats.insert(vn, Box::new(dispatch));
    }

    fn allocate_promise_resolver_pair(&mut self) -> (KernelPromiseID, KernelResolverID) {
        let id = self.next_promise_resolver_id;
        self.next_promise_resolver_id += 1;
        (KernelPromiseID(id), KernelResolverID(id))
    }

    pub fn push(&mut self, name: &VatName, export: KernelExportID, method: String) {
        let (_pid, rid) = self.allocate_promise_resolver_pair();
        let pd = PendingDelivery {
            target: Target::Export(VatName(name.0.clone()), export),
            method: method,
            args: 0,
            resolver: rid,
        };
        self.run_queue.push_back(pd);
    }

    fn map_export_target(&self, id: KernelExportID) -> VatExportID {
        VatExportID(id.0)
    }

    fn _map_inbound(&mut self, _vn: &VatName, id: KernelExportID) -> VatExportID {
        // todo clist mapping
        VatExportID(id.0)
    }

    fn process(&mut self, pd: PendingDelivery) {
        let t = pd.target;
        println!("process: {}.{}", t, pd.method);
        match t {
            Target::Export(vn, kid) => {
                //let vid = self.map_inbound(&vn, kid);
                let vid = self.map_export_target(kid);
                let dispatch = self.vats.get(&vn).unwrap();
                let mut syscall: Box<dyn Syscall> =
                    Box::new(VatSyscall::new(&mut self.run_queue));
                dispatch.deliver(&mut syscall, vid);
            }
            //Target::Promise(_pid) => {}
            _ => panic!(),
        };
    }

    pub fn step(&mut self) {
        println!("kernel.step");
        match self.run_queue.pop_front() {
            Some(pd) => self.process(pd),
            None => (),
        };
    }

    pub fn run(&mut self) {
        println!("kernel.run");
    }
}

pub enum UsizeOrU64 {
    Usize(usize),
    U64(u64),
}

impl UsizeOrU64 {
    pub fn usize(num: usize) -> Self { Self::Usize(num) }
    pub fn u64(num: u64) -> Self { Self::U64(num) }
}

impl PartialEq for UsizeOrU64 {
    fn eq(&self, other: &Self) -> bool {
        binary_op(self, other, usize::eq, u64::eq)
    }
}

impl Eq for UsizeOrU64 {}

impl PartialOrd for UsizeOrU64 {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for UsizeOrU64 {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        binary_op(self, other, usize::cmp, u64::cmp)
    }
}

fn binary_op<F, G, T>(this: &UsizeOrU64, other: &UsizeOrU64, usize_op: F, u64_op: G) -> T
where
    F: FnOnce(&usize, &usize) -> T,
    G: FnOnce(&u64, &u64) -> T,
{
    let (usize_num, u64_num, order) = match (this, other) {
        (UsizeOrU64::Usize(a), UsizeOrU64::Usize(b)) => return usize_op(a, b),
        (UsizeOrU64::U64(a), UsizeOrU64::U64(b)) => return u64_op(a, b),
        (UsizeOrU64::Usize(usize_num), UsizeOrU64::U64(u64_num)) => (usize_num.to_owned(), u64_num.to_owned(), true),
        (UsizeOrU64::U64(u64_num), UsizeOrU64::Usize(usize_num)) => (usize_num.to_owned(), u64_num.to_owned(), false),
    };

    if core::mem::size_of::<usize>() < core::mem::size_of::<u64>() {
        let upcasted = usize_num as u64;
        return if order { u64_op(&upcasted, &u64_num) } else { u64_op(&u64_num, &upcasted) }
    }
    let upcasted = u64_num as usize;
    if order { usize_op(&usize_num, &upcasted) } else { usize_op(&upcasted, &usize_num) }
}

pub struct IntSet {
    sparse: Vec<usize>,
    dense: Vec<usize>,
}

impl IntSet {
    pub fn new(capacity: usize) -> Self {
        Self {
            sparse: vec![usize::MAX; capacity],
            dense: Vec::with_capacity(capacity),
        }
    }

    pub fn insert(&mut self, value: usize) -> bool {
        let idx = self.sparse[value];
        if self.dense.get(idx) != Some(&value) {
            self.sparse[value] = self.dense.len();
            self.dense.push(value);
            true
        } else {
            false
        }
    }

    pub fn len(&self) -> usize {
        self.dense.len()
    }

    pub fn into_vec(self) -> Vec<usize> {
        self.dense
    }
}

impl std::ops::Index<usize> for IntSet {
    type Output = usize;

    fn index(&self, index: usize) -> &Self::Output {
        self.dense.index(index)
    }
}

impl std::iter::Extend<usize> for IntSet {
    fn extend<T: IntoIterator<Item = usize>>(&mut self, iter: T) {
        for val in iter {
            self.insert(val);
        }
    }
}

impl<'a> std::iter::Extend<&'a usize> for IntSet {
    fn extend<T: IntoIterator<Item = &'a usize>>(&mut self, iter: T) {
        for val in iter {
            self.insert(*val);
        }
    }
}

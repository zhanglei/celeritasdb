use super::open;
use crate::DBColumnFamily;
use crate::WriteEntry;
use crate::{Base, RocksDBEngine, StorageError};
use rocksdb::{CFHandle, SeekKey, Writable, WriteBatch};

impl RocksDBEngine {
    /// Open a Engine base on rocksdb to use snapshot.
    ///
    /// # Examples:
    /// ```
    /// use tempfile::Builder;
    /// use crate::storage::RocksDBEngine;
    ///
    /// let tmp_root = Builder::new().tempdir().unwrap();
    /// let db_path = format!("{}/test", tmp_root.path().display());
    ///
    /// let my_eng;
    /// match RocksDBEngine::new(&db_path) {
    ///     Ok(eng) => my_eng = eng,
    ///     Err(err) => println!("failed to get rocksdb engine, failed: {}", err),
    /// };
    /// ```
    pub fn new(path: &str) -> Result<RocksDBEngine, StorageError> {
        let db = open(path)?;

        Ok(RocksDBEngine { db })
    }

    /// make rocksdb column family handle
    fn _make_cf_handle(&self, cf: DBColumnFamily) -> Result<&CFHandle, StorageError> {
        match self.db.cf_handle(cf.into()) {
            Some(h) => Ok(h),
            None => Err(format!("got column family {:?} handle failed", cf).into()),
        }
    }

    fn _range(
        &self,
        cf: DBColumnFamily,
        key: &Vec<u8>,
        include: bool,
        reverse: bool,
    ) -> Option<(Vec<u8>, Vec<u8>)> {
        let cf = self._make_cf_handle(cf).ok()?;
        let mut iter = self.db.iter_cf(cf);

        iter.seek(SeekKey::from(&key[..]));
        if !iter.valid() {
            // TODO may be a rocksdb panic here
            return None;
        }

        match iter.kv() {
            Some(kv) => {
                if include {
                    return Some(kv);
                };

                if &kv.0 != key {
                    return Some(kv);
                };
            }
            None => return None,
        }

        if reverse {
            iter.prev();
        } else {
            iter.next();
        }
        if !iter.valid() {
            // TODO may be a rocksdb panic here
            return None;
        }

        return iter.kv();
    }
}

impl Base for RocksDBEngine {
    fn set(&self, cf: DBColumnFamily, key: &Vec<u8>, value: &Vec<u8>) -> Result<(), StorageError> {
        let cfh = self._make_cf_handle(cf)?;
        Ok(self.db.put_cf(cfh, key, value)?)
    }

    fn get(&self, cf: DBColumnFamily, key: &Vec<u8>) -> Result<Option<Vec<u8>>, StorageError> {
        let cfh = self._make_cf_handle(cf)?;
        let r = self.db.get_cf(cfh, key)?;
        Ok(r.map(|x| x.to_vec()))
    }

    fn delete(&self, cf: DBColumnFamily, key: &Vec<u8>) -> Result<(), StorageError> {
        let cfh = self._make_cf_handle(cf)?;
        Ok(self.db.delete_cf(cfh, key)?)
    }

    fn next(&self, cf: DBColumnFamily, key: &Vec<u8>, include: bool) -> Option<(Vec<u8>, Vec<u8>)> {
        self._range(cf, key, include, false)
    }

    fn prev(&self, cf: DBColumnFamily, key: &Vec<u8>, include: bool) -> Option<(Vec<u8>, Vec<u8>)> {
        self._range(cf, key, include, true)
    }

    fn write_batch(&self, entrys: &Vec<WriteEntry>) -> Result<(), StorageError> {
        let batch = WriteBatch::with_capacity(entrys.len());
        for en in entrys {
            match en {
                WriteEntry::Nil => {}
                WriteEntry::Set(cf, k, v) => {
                    let cfh = self._make_cf_handle(*cf)?;
                    batch.put_cf(cfh, k, v)?;
                }
                WriteEntry::Delete(cf, k) => {
                    let cfh = self._make_cf_handle(*cf)?;
                    batch.delete_cf(cfh, k)?;
                }
            }
        }

        Ok(self.db.write(batch)?)
    }
}

#[cfg(test)]
mod tests {
    use crate::test_engine::*;
    use crate::*;
    use tempfile::Builder;

    fn new_eng() -> RocksDBEngine {
        let tmp_root = Builder::new().tempdir().unwrap();
        let db_path = format!("{}/test", tmp_root.path().display());

        RocksDBEngine::new(&db_path).unwrap()
    }

    #[test]
    fn test_engine() {
        {
            let eng = new_eng();
            test_base_trait(&eng);
        }

        {
            let eng = new_eng();
            test_kv_trait(&eng);
        }

        {
            let eng = new_eng();
            test_columned_trait(&eng);
        }

        {
            let eng = new_eng();
            test_instance_trait(&eng);
        }
    }
}

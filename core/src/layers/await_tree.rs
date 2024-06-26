// Licensed to the Apache Software Foundation (ASF) under one
// or more contributor license agreements.  See the NOTICE file
// distributed with this work for additional information
// regarding copyright ownership.  The ASF licenses this file
// to you under the Apache License, Version 2.0 (the
// "License"); you may not use this file except in compliance
// with the License.  You may obtain a copy of the License at
//
//   http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing,
// software distributed under the License is distributed on an
// "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY
// KIND, either express or implied.  See the License for the
// specific language governing permissions and limitations
// under the License.

use await_tree::InstrumentAwait;
use futures::{Future, FutureExt};
use oio::{ListOperation, ReadOperation, WriteOperation};

use crate::raw::*;
use crate::*;

/// Add a Instrument await-tree for actor-based applications to the underlying services.
///
/// # AwaitTree
///
/// await-tree allows developers to dump this execution tree at runtime,
/// with the span of each Future annotated by instrument_await.
/// Read more about [await-tree](https://docs.rs/await-tree/latest/await_tree/)
///
/// # Examples
///
/// ```no_run
/// use anyhow::Result;
/// use opendal::layers::AwaitTreeLayer;
/// use opendal::services;
/// use opendal::Operator;
/// use opendal::Scheme;
///
/// let _ = Operator::new(services::Memory::default())
///     .expect("must init")
///     .layer(AwaitTreeLayer::new())
///     .finish();
/// ```
#[derive(Clone, Default)]
pub struct AwaitTreeLayer {}

impl AwaitTreeLayer {
    /// Create a new `AwaitTreeLayer`.
    pub fn new() -> Self {
        Self {}
    }
}

impl<A: Access> Layer<A> for AwaitTreeLayer {
    type LayeredAccess = AwaitTreeAccessor<A>;

    fn layer(&self, accessor: A) -> Self::LayeredAccess {
        AwaitTreeAccessor { inner: accessor }
    }
}

#[derive(Debug, Clone)]
pub struct AwaitTreeAccessor<A: Access> {
    inner: A,
}

impl<A: Access> LayeredAccess for AwaitTreeAccessor<A> {
    type Inner = A;
    type Reader = AwaitTreeWrapper<A::Reader>;
    type BlockingReader = AwaitTreeWrapper<A::BlockingReader>;
    type Writer = AwaitTreeWrapper<A::Writer>;
    type BlockingWriter = AwaitTreeWrapper<A::BlockingWriter>;
    type Lister = AwaitTreeWrapper<A::Lister>;
    type BlockingLister = AwaitTreeWrapper<A::BlockingLister>;

    fn inner(&self) -> &Self::Inner {
        &self.inner
    }

    async fn read(&self, path: &str, args: OpRead) -> Result<(RpRead, Self::Reader)> {
        self.inner
            .read(path, args)
            .instrument_await(format!("opendal::{}", Operation::Read))
            .map(|v| v.map(|(rp, r)| (rp, AwaitTreeWrapper::new(r))))
            .await
    }

    async fn write(&self, path: &str, args: OpWrite) -> Result<(RpWrite, Self::Writer)> {
        self.inner
            .write(path, args)
            .instrument_await(format!("opendal::{}", Operation::Write))
            .map(|v| v.map(|(rp, r)| (rp, AwaitTreeWrapper::new(r))))
            .await
    }

    async fn copy(&self, from: &str, to: &str, args: OpCopy) -> Result<RpCopy> {
        self.inner()
            .copy(from, to, args)
            .instrument_await(format!("opendal::{}", Operation::Copy))
            .await
    }

    async fn rename(&self, from: &str, to: &str, args: OpRename) -> Result<RpRename> {
        self.inner()
            .rename(from, to, args)
            .instrument_await(format!("opendal::{}", Operation::Rename))
            .await
    }

    async fn stat(&self, path: &str, args: OpStat) -> Result<RpStat> {
        self.inner
            .stat(path, args)
            .instrument_await(format!("opendal::{}", Operation::Stat))
            .await
    }

    async fn delete(&self, path: &str, args: OpDelete) -> Result<RpDelete> {
        self.inner
            .delete(path, args)
            .instrument_await(format!("opendal::{}", Operation::Delete))
            .await
    }

    async fn list(&self, path: &str, args: OpList) -> Result<(RpList, Self::Lister)> {
        self.inner
            .list(path, args)
            .instrument_await(format!("opendal::{}", Operation::List))
            .map(|v| v.map(|(rp, r)| (rp, AwaitTreeWrapper::new(r))))
            .await
    }

    async fn presign(&self, path: &str, args: OpPresign) -> Result<RpPresign> {
        self.inner
            .presign(path, args)
            .instrument_await(format!("opendal::{}", Operation::Presign))
            .await
    }

    async fn batch(&self, args: OpBatch) -> Result<RpBatch> {
        self.inner
            .batch(args)
            .instrument_await(format!("opendal::{}", Operation::Batch))
            .await
    }

    fn blocking_read(&self, path: &str, args: OpRead) -> Result<(RpRead, Self::BlockingReader)> {
        self.inner
            .blocking_read(path, args)
            .map(|(rp, r)| (rp, AwaitTreeWrapper::new(r)))
    }

    fn blocking_write(&self, path: &str, args: OpWrite) -> Result<(RpWrite, Self::BlockingWriter)> {
        self.inner
            .blocking_write(path, args)
            .map(|(rp, r)| (rp, AwaitTreeWrapper::new(r)))
    }

    fn blocking_list(&self, path: &str, args: OpList) -> Result<(RpList, Self::BlockingLister)> {
        self.inner
            .blocking_list(path, args)
            .map(|(rp, r)| (rp, AwaitTreeWrapper::new(r)))
    }
}

pub struct AwaitTreeWrapper<R> {
    inner: R,
}

impl<R> AwaitTreeWrapper<R> {
    fn new(inner: R) -> Self {
        Self { inner }
    }
}

impl<R: oio::Read> oio::Read for AwaitTreeWrapper<R> {
    async fn read(&mut self) -> Result<Buffer> {
        self.inner
            .read()
            .instrument_await(format!("opendal::{}", ReadOperation::Read))
            .await
    }
}

impl<R: oio::BlockingRead> oio::BlockingRead for AwaitTreeWrapper<R> {
    fn read(&mut self) -> Result<Buffer> {
        self.inner.read()
    }
}

impl<R: oio::Write> oio::Write for AwaitTreeWrapper<R> {
    fn write(&mut self, bs: Buffer) -> impl Future<Output = Result<usize>> + MaybeSend {
        self.inner
            .write(bs)
            .instrument_await(format!("opendal::{}", WriteOperation::Write.into_static()))
    }

    fn abort(&mut self) -> impl Future<Output = Result<()>> + MaybeSend {
        self.inner
            .abort()
            .instrument_await(format!("opendal::{}", WriteOperation::Abort.into_static()))
    }

    fn close(&mut self) -> impl Future<Output = Result<()>> + MaybeSend {
        self.inner
            .close()
            .instrument_await(format!("opendal::{}", WriteOperation::Close.into_static()))
    }
}

impl<R: oio::BlockingWrite> oio::BlockingWrite for AwaitTreeWrapper<R> {
    fn write(&mut self, bs: Buffer) -> Result<usize> {
        self.inner.write(bs)
    }

    fn close(&mut self) -> Result<()> {
        self.inner.close()
    }
}

impl<R: oio::List> oio::List for AwaitTreeWrapper<R> {
    async fn next(&mut self) -> Result<Option<oio::Entry>> {
        self.inner
            .next()
            .instrument_await(format!("opendal::{}", ListOperation::Next))
            .await
    }
}

impl<R: oio::BlockingList> oio::BlockingList for AwaitTreeWrapper<R> {
    fn next(&mut self) -> Result<Option<oio::Entry>> {
        self.inner.next()
    }
}

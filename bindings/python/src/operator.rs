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

use std::collections::HashMap;
use std::str::FromStr;
use std::time::Duration;

use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::PyBytes;
use pyo3::types::PyDict;
use pyo3_asyncio::tokio::future_into_py;

use crate::*;

fn build_operator(
    scheme: ocore::Scheme,
    map: HashMap<String, String>,
) -> PyResult<ocore::Operator> {
    let mut op = ocore::Operator::via_map(scheme, map).map_err(format_pyerr)?;
    if !op.info().full_capability().blocking {
        let runtime = pyo3_asyncio::tokio::get_runtime();
        let _guard = runtime.enter();
        op = op
            .layer(ocore::layers::BlockingLayer::create().expect("blocking layer must be created"));
    }

    Ok(op)
}

/// `Operator` is the entry for all public blocking APIs
///
/// Create a new blocking `Operator` with the given `scheme` and options(`**kwargs`).
#[pyclass(module = "opendal")]
pub struct Operator(ocore::BlockingOperator);

#[pymethods]
impl Operator {
    #[new]
    #[pyo3(signature = (scheme, *, **map))]
    pub fn new(scheme: &str, map: Option<&Bound<PyDict>>) -> PyResult<Self> {
        let scheme = ocore::Scheme::from_str(scheme)
            .map_err(|err| {
                ocore::Error::new(ocore::ErrorKind::Unexpected, "unsupported scheme")
                    .set_source(err)
            })
            .map_err(format_pyerr)?;
        let map = map
            .map(|v| {
                v.extract::<HashMap<String, String>>()
                    .expect("must be valid hashmap")
            })
            .unwrap_or_default();

        Ok(Operator(build_operator(scheme, map)?.blocking()))
    }

    /// Add new layers upon existing operator
    pub fn layer(&self, layer: &layers::Layer) -> PyResult<Self> {
        let op = layer.0.layer(self.0.clone().into());
        Ok(Self(op.blocking()))
    }

    /// Open a file-like reader for the given path.
    pub fn open(&self, path: String, mode: String) -> PyResult<File> {
        let this = self.0.clone();
        let capability = self.capability()?;
        if mode == "rb" {
            let r = this
                .reader(&path)
                .map_err(format_pyerr)?
                .into_std_read(..)
                .map_err(format_pyerr)?;
            Ok(File::new_reader(r, capability))
        } else if mode == "wb" {
            let w = this.writer(&path).map_err(format_pyerr)?;
            Ok(File::new_writer(w, capability))
        } else {
            Err(UnsupportedError::new_err(format!(
                "OpenDAL doesn't support mode: {mode}"
            )))
        }
    }

    /// Read the whole path into bytes.
    pub fn read<'p>(&'p self, py: Python<'p>, path: &str) -> PyResult<Bound<PyAny>> {
        let buffer = self.0.read(path).map_err(format_pyerr)?.to_vec();
        Buffer::new(buffer).into_bytes_ref(py)
    }

    /// Write bytes into given path.
    #[pyo3(signature = (path, bs, **kwargs))]
    pub fn write(&self, path: &str, bs: Vec<u8>, kwargs: Option<&Bound<PyDict>>) -> PyResult<()> {
        let opwrite = build_opwrite(kwargs)?;
        let mut write = self.0.write_with(path, bs).append(opwrite.append());
        if let Some(chunk) = opwrite.chunk() {
            write = write.chunk(chunk);
        }
        if let Some(content_type) = opwrite.content_type() {
            write = write.content_type(content_type);
        }
        if let Some(content_disposition) = opwrite.content_disposition() {
            write = write.content_disposition(content_disposition);
        }
        if let Some(cache_control) = opwrite.cache_control() {
            write = write.cache_control(cache_control);
        }

        write.call().map_err(format_pyerr)
    }

    /// Get current path's metadata **without cache** directly.
    pub fn stat(&self, path: &str) -> PyResult<Metadata> {
        self.0.stat(path).map_err(format_pyerr).map(Metadata::new)
    }

    /// Copy source to target.
    pub fn copy(&self, source: &str, target: &str) -> PyResult<()> {
        self.0.copy(source, target).map_err(format_pyerr)
    }

    /// Rename filename.
    pub fn rename(&self, source: &str, target: &str) -> PyResult<()> {
        self.0.rename(source, target).map_err(format_pyerr)
    }

    /// Remove all file
    pub fn remove_all(&self, path: &str) -> PyResult<()> {
        self.0.remove_all(path).map_err(format_pyerr)
    }

    /// Create a dir at given path.
    ///
    /// # Notes
    ///
    /// To indicate that a path is a directory, it is compulsory to include
    /// a trailing / in the path. Failure to do so may result in
    /// `NotADirectory` error being returned by OpenDAL.
    ///
    /// # Behavior
    ///
    /// - Create on existing dir will succeed.
    /// - Create dir is always recursive, works like `mkdir -p`
    pub fn create_dir(&self, path: &str) -> PyResult<()> {
        self.0.create_dir(path).map_err(format_pyerr)
    }

    /// Delete given path.
    ///
    /// # Notes
    ///
    /// - Delete not existing error won't return errors.
    pub fn delete(&self, path: &str) -> PyResult<()> {
        self.0.delete(path).map_err(format_pyerr)
    }

    /// List current dir path.
    pub fn list(&self, path: &str) -> PyResult<BlockingLister> {
        let l = self.0.lister(path).map_err(format_pyerr)?;
        Ok(BlockingLister::new(l))
    }

    /// List dir in flat way.
    pub fn scan(&self, path: &str) -> PyResult<BlockingLister> {
        let l = self
            .0
            .lister_with(path)
            .recursive(true)
            .call()
            .map_err(format_pyerr)?;
        Ok(BlockingLister::new(l))
    }

    pub fn capability(&self) -> PyResult<capability::Capability> {
        Ok(capability::Capability::new(self.0.info().full_capability()))
    }

    pub fn to_async_operator(&self) -> PyResult<AsyncOperator> {
        Ok(AsyncOperator(self.0.clone().into()))
    }

    fn __repr__(&self) -> String {
        let info = self.0.info();
        let name = info.name();
        if name.is_empty() {
            format!("Operator(\"{}\", root=\"{}\")", info.scheme(), info.root())
        } else {
            format!(
                "Operator(\"{}\", root=\"{}\", name=\"{name}\")",
                info.scheme(),
                info.root()
            )
        }
    }
}

/// `AsyncOperator` is the entry for all public async APIs
///
/// Create a new `AsyncOperator` with the given `scheme` and options(`**kwargs`).
#[pyclass(module = "opendal")]
pub struct AsyncOperator(ocore::Operator);

#[pymethods]
impl AsyncOperator {
    #[new]
    #[pyo3(signature = (scheme, *,  **map))]
    pub fn new(scheme: &str, map: Option<&Bound<PyDict>>) -> PyResult<Self> {
        let scheme = ocore::Scheme::from_str(scheme)
            .map_err(|err| {
                ocore::Error::new(ocore::ErrorKind::Unexpected, "unsupported scheme")
                    .set_source(err)
            })
            .map_err(format_pyerr)?;
        let map = map
            .map(|v| {
                v.extract::<HashMap<String, String>>()
                    .expect("must be valid hashmap")
            })
            .unwrap_or_default();

        Ok(AsyncOperator(build_operator(scheme, map)?))
    }

    /// Add new layers upon existing operator
    pub fn layer(&self, layer: &layers::Layer) -> PyResult<Self> {
        let op = layer.0.layer(self.0.clone());
        Ok(Self(op))
    }

    /// Open a file-like reader for the given path.
    pub fn open<'p>(
        &'p self,
        py: Python<'p>,
        path: String,
        mode: String,
    ) -> PyResult<Bound<PyAny>> {
        let this = self.0.clone();
        let capability = self.capability()?;

        future_into_py(py, async move {
            if mode == "rb" {
                let r = this
                    .reader(&path)
                    .await
                    .map_err(format_pyerr)?
                    .into_futures_async_read(..)
                    .await
                    .map_err(format_pyerr)?;
                Ok(AsyncFile::new_reader(r, capability))
            } else if mode == "wb" {
                let w = this.writer(&path).await.map_err(format_pyerr)?;
                Ok(AsyncFile::new_writer(w, capability))
            } else {
                Err(UnsupportedError::new_err(format!(
                    "OpenDAL doesn't support mode: {mode}"
                )))
            }
        })
    }

    /// Read the whole path into bytes.
    pub fn read<'p>(&'p self, py: Python<'p>, path: String) -> PyResult<Bound<PyAny>> {
        let this = self.0.clone();
        future_into_py(py, async move {
            let res: Vec<u8> = this.read(&path).await.map_err(format_pyerr)?.to_vec();
            Python::with_gil(|py| Buffer::new(res).into_bytes(py))
        })
    }

    /// Write bytes into given path.
    #[pyo3(signature = (path, bs, **kwargs))]
    pub fn write<'p>(
        &'p self,
        py: Python<'p>,
        path: String,
        bs: &Bound<PyBytes>,
        kwargs: Option<&Bound<PyDict>>,
    ) -> PyResult<Bound<PyAny>> {
        let opwrite = build_opwrite(kwargs)?;
        let this = self.0.clone();
        let bs = bs.as_bytes().to_vec();
        future_into_py(py, async move {
            let mut write = this.write_with(&path, bs).append(opwrite.append());
            if let Some(buffer) = opwrite.chunk() {
                write = write.chunk(buffer);
            }
            if let Some(content_type) = opwrite.content_type() {
                write = write.content_type(content_type);
            }
            if let Some(content_disposition) = opwrite.content_disposition() {
                write = write.content_disposition(content_disposition);
            }
            if let Some(cache_control) = opwrite.cache_control() {
                write = write.cache_control(cache_control);
            }
            write.await.map_err(format_pyerr)
        })
    }

    /// Get current path's metadata **without cache** directly.
    pub fn stat<'p>(&'p self, py: Python<'p>, path: String) -> PyResult<Bound<PyAny>> {
        let this = self.0.clone();
        future_into_py(py, async move {
            let res: Metadata = this
                .stat(&path)
                .await
                .map_err(format_pyerr)
                .map(Metadata::new)?;

            Ok(res)
        })
    }

    /// Copy source to target.``
    pub fn copy<'p>(
        &'p self,
        py: Python<'p>,
        source: String,
        target: String,
    ) -> PyResult<Bound<PyAny>> {
        let this = self.0.clone();
        future_into_py(py, async move {
            this.copy(&source, &target).await.map_err(format_pyerr)
        })
    }

    /// Rename filename
    pub fn rename<'p>(
        &'p self,
        py: Python<'p>,
        source: String,
        target: String,
    ) -> PyResult<Bound<PyAny>> {
        let this = self.0.clone();
        future_into_py(py, async move {
            this.rename(&source, &target).await.map_err(format_pyerr)
        })
    }

    /// Remove all file
    pub fn remove_all<'p>(&'p self, py: Python<'p>, path: String) -> PyResult<Bound<PyAny>> {
        let this = self.0.clone();
        future_into_py(py, async move {
            this.remove_all(&path).await.map_err(format_pyerr)
        })
    }

    /// Create a dir at given path.
    ///
    /// # Notes
    ///
    /// To indicate that a path is a directory, it is compulsory to include
    /// a trailing / in the path. Failure to do so may result in
    /// `NotADirectory` error being returned by OpenDAL.
    ///
    /// # Behavior
    ///
    /// - Create on existing dir will succeed.
    /// - Create dir is always recursive, works like `mkdir -p`
    pub fn create_dir<'p>(&'p self, py: Python<'p>, path: String) -> PyResult<Bound<PyAny>> {
        let this = self.0.clone();
        future_into_py(py, async move {
            this.create_dir(&path).await.map_err(format_pyerr)
        })
    }

    /// Delete given path.
    ///
    /// # Notes
    ///
    /// - Delete not existing error won't return errors.
    pub fn delete<'p>(&'p self, py: Python<'p>, path: String) -> PyResult<Bound<PyAny>> {
        let this = self.0.clone();
        future_into_py(
            py,
            async move { this.delete(&path).await.map_err(format_pyerr) },
        )
    }

    /// List current dir path.
    pub fn list<'p>(&'p self, py: Python<'p>, path: String) -> PyResult<Bound<PyAny>> {
        let this = self.0.clone();
        future_into_py(py, async move {
            let lister = this.lister(&path).await.map_err(format_pyerr)?;
            let pylister: PyObject = Python::with_gil(|py| AsyncLister::new(lister).into_py(py));
            Ok(pylister)
        })
    }

    /// List dir in flat way.
    pub fn scan<'p>(&'p self, py: Python<'p>, path: String) -> PyResult<Bound<PyAny>> {
        let this = self.0.clone();
        future_into_py(py, async move {
            let lister = this
                .lister_with(&path)
                .recursive(true)
                .await
                .map_err(format_pyerr)?;
            let pylister: PyObject = Python::with_gil(|py| AsyncLister::new(lister).into_py(py));
            Ok(pylister)
        })
    }

    /// Presign an operation for stat(head) which expires after `expire_second` seconds.
    pub fn presign_stat<'p>(
        &'p self,
        py: Python<'p>,
        path: String,
        expire_second: u64,
    ) -> PyResult<Bound<PyAny>> {
        let this = self.0.clone();
        future_into_py(py, async move {
            let res = this
                .presign_stat(&path, Duration::from_secs(expire_second))
                .await
                .map_err(format_pyerr)
                .map(PresignedRequest)?;

            Ok(res)
        })
    }

    /// Presign an operation for read which expires after `expire_second` seconds.
    pub fn presign_read<'p>(
        &'p self,
        py: Python<'p>,
        path: String,
        expire_second: u64,
    ) -> PyResult<Bound<PyAny>> {
        let this = self.0.clone();
        future_into_py(py, async move {
            let res = this
                .presign_read(&path, Duration::from_secs(expire_second))
                .await
                .map_err(format_pyerr)
                .map(PresignedRequest)?;

            Ok(res)
        })
    }

    /// Presign an operation for write which expires after `expire_second` seconds.
    pub fn presign_write<'p>(
        &'p self,
        py: Python<'p>,
        path: String,
        expire_second: u64,
    ) -> PyResult<Bound<PyAny>> {
        let this = self.0.clone();
        future_into_py(py, async move {
            let res = this
                .presign_write(&path, Duration::from_secs(expire_second))
                .await
                .map_err(format_pyerr)
                .map(PresignedRequest)?;

            Ok(res)
        })
    }

    pub fn capability(&self) -> PyResult<capability::Capability> {
        Ok(capability::Capability::new(self.0.info().full_capability()))
    }

    pub fn to_operator(&self) -> PyResult<Operator> {
        Ok(Operator(self.0.clone().blocking()))
    }

    fn __repr__(&self) -> String {
        let info = self.0.info();
        let name = info.name();
        if name.is_empty() {
            format!(
                "AsyncOperator(\"{}\", root=\"{}\")",
                info.scheme(),
                info.root()
            )
        } else {
            format!(
                "AsyncOperator(\"{}\", root=\"{}\", name=\"{name}\")",
                info.scheme(),
                info.root()
            )
        }
    }
}

/// recognize OpWrite-equivalent options passed as python dict
pub(crate) fn build_opwrite(kwargs: Option<&Bound<PyDict>>) -> PyResult<ocore::raw::OpWrite> {
    use ocore::raw::OpWrite;
    let mut op = OpWrite::new();

    let dict = if let Some(kwargs) = kwargs {
        kwargs
    } else {
        return Ok(op);
    };

    if let Some(append) = dict.get_item("append")? {
        let v = append
            .extract::<bool>()
            .map_err(|err| PyValueError::new_err(format!("append must be bool, got {}", err)))?;
        op = op.with_append(v);
    }

    if let Some(chunk) = dict.get_item("chunk")? {
        let v = chunk
            .extract::<usize>()
            .map_err(|err| PyValueError::new_err(format!("chunk must be usize, got {}", err)))?;
        op = op.with_chunk(v);
    }

    if let Some(content_type) = dict.get_item("content_type")? {
        let v = content_type.extract::<String>().map_err(|err| {
            PyValueError::new_err(format!("content_type must be str, got {}", err))
        })?;
        op = op.with_content_type(v.as_str());
    }

    if let Some(content_disposition) = dict.get_item("content_disposition")? {
        let v = content_disposition.extract::<String>().map_err(|err| {
            PyValueError::new_err(format!("content_disposition must be str, got {}", err))
        })?;
        op = op.with_content_disposition(v.as_str());
    }

    if let Some(cache_control) = dict.get_item("cache_control")? {
        let v = cache_control.extract::<String>().map_err(|err| {
            PyValueError::new_err(format!("cache_control must be str, got {}", err))
        })?;
        op = op.with_cache_control(v.as_str());
    }

    Ok(op)
}

#[pyclass(module = "opendal")]
pub struct PresignedRequest(ocore::raw::PresignedRequest);

#[pymethods]
impl PresignedRequest {
    /// Return the URL of this request.
    #[getter]
    pub fn url(&self) -> String {
        self.0.uri().to_string()
    }

    /// Return the HTTP method of this request.
    #[getter]
    pub fn method(&self) -> &str {
        self.0.method().as_str()
    }

    /// Return the HTTP headers of this request.
    #[getter]
    pub fn headers(&self) -> PyResult<HashMap<&str, &str>> {
        let mut headers = HashMap::new();
        for (k, v) in self.0.header().iter() {
            let k = k.as_str();
            let v = v
                .to_str()
                .map_err(|err| UnexpectedError::new_err(err.to_string()))?;
            if headers.insert(k, v).is_some() {
                return Err(UnexpectedError::new_err("duplicate header"));
            }
        }
        Ok(headers)
    }
}

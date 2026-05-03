use std::ffi::c_char;

const ERROR_BUFFER_LEN: usize = 512;

#[derive(Debug)]
pub enum VectorError {
    EmptyVector,
    LengthMismatch { left: usize, right: usize },
    InvalidTopK { requested: usize, available: usize },
    NativeError(String),
}

impl std::fmt::Display for VectorError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VectorError::EmptyVector => write!(f, "vector must not be empty"),
            VectorError::LengthMismatch { left, right } => {
                write!(f, "vector length mismatch: left={left}, right={right}")
            }
            VectorError::InvalidTopK {
                requested,
                available,
            } => write!(
                f,
                "invalid top_k request: requested={requested}, available={available}"
            ),
            VectorError::NativeError(message) => write!(f, "{message}"),
        }
    }
}

impl std::error::Error for VectorError {}

type Result<T> = std::result::Result<T, VectorError>;

#[link(name = "general_wheel_cpp", kind = "static")]
unsafe extern "C" {
    fn gw_dot_product(
        left: *const f32,
        right: *const f32,
        size: usize,
        out: *mut f32,
        err: *mut c_char,
        err_len: usize,
    ) -> bool;
    fn gw_cosine_similarity(
        left: *const f32,
        right: *const f32,
        size: usize,
        out: *mut f32,
        err: *mut c_char,
        err_len: usize,
    ) -> bool;
    fn gw_top_k_similar(
        vectors: *const *const f32,
        ref_vector: *const f32,
        vec_size: usize,
        collection_size: usize,
        k: usize,
        out_indices: *mut usize,
        out_scores: *mut f32,
        err: *mut c_char,
        err_len: usize,
    ) -> bool;
}

pub fn dot_product(left: &[f32], right: &[f32]) -> Result<f32> {
    validate_pair(left, right)?;
    call_scalar(left, right, gw_dot_product)
}

pub fn cosine_similarity(left: &[f32], right: &[f32]) -> Result<f32> {
    validate_pair(left, right)?;
    call_scalar(left, right, gw_cosine_similarity)
}

pub fn top_k_similar(
    candidates: &[Vec<f32>],
    query: &[f32],
    top_k: usize,
) -> Result<Vec<(usize, f32)>> {
    if query.is_empty() {
        return Err(VectorError::EmptyVector);
    }
    if candidates.is_empty() {
        return Err(VectorError::InvalidTopK {
            requested: top_k,
            available: 0,
        });
    }
    if top_k == 0 || top_k > candidates.len() {
        return Err(VectorError::InvalidTopK {
            requested: top_k,
            available: candidates.len(),
        });
    }

    for candidate in candidates {
        validate_pair(candidate, query)?;
    }

    let pointers: Vec<*const f32> = candidates.iter().map(|item| item.as_ptr()).collect();
    let mut indices = vec![0usize; top_k];
    let mut scores = vec![0f32; top_k];
    let mut err_buf = vec![0i8; ERROR_BUFFER_LEN];

    let ok = unsafe {
        gw_top_k_similar(
            pointers.as_ptr(),
            query.as_ptr(),
            query.len(),
            candidates.len(),
            top_k,
            indices.as_mut_ptr(),
            scores.as_mut_ptr(),
            err_buf.as_mut_ptr(),
            err_buf.len(),
        )
    };

    if !ok {
        return Err(VectorError::NativeError(decode_error(&err_buf)));
    }

    Ok(indices.into_iter().zip(scores).collect())
}

fn validate_pair(left: &[f32], right: &[f32]) -> Result<()> {
    if left.is_empty() || right.is_empty() {
        return Err(VectorError::EmptyVector);
    }
    if left.len() != right.len() {
        return Err(VectorError::LengthMismatch {
            left: left.len(),
            right: right.len(),
        });
    }
    Ok(())
}

fn call_scalar(
    left: &[f32],
    right: &[f32],
    callback: unsafe extern "C" fn(
        *const f32,
        *const f32,
        usize,
        *mut f32,
        *mut c_char,
        usize,
    ) -> bool,
) -> Result<f32> {
    let mut out = 0f32;
    let mut err_buf = vec![0i8; ERROR_BUFFER_LEN];
    let ok = unsafe {
        callback(
            left.as_ptr(),
            right.as_ptr(),
            left.len(),
            &mut out,
            err_buf.as_mut_ptr(),
            err_buf.len(),
        )
    };

    if ok {
        Ok(out)
    } else {
        Err(VectorError::NativeError(decode_error(&err_buf)))
    }
}

fn decode_error(buffer: &[i8]) -> String {
    let bytes: Vec<u8> = buffer
        .iter()
        .copied()
        .take_while(|byte| *byte != 0)
        .map(|byte| byte as u8)
        .collect();
    if bytes.is_empty() {
        "general-wheel-cpp call failed".to_string()
    } else {
        String::from_utf8_lossy(&bytes).into_owned()
    }
}

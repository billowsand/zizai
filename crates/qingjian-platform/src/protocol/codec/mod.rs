//! 线上帧格式：4 字节小端长度前缀 + JSON 消息体。Server 与 TSF DLL 两端共用这套读写，
//! 所以放在平台层而不是某个 app 里。传输本身（命名管道等）在 app 侧，这里只切帧。

mod error;
mod incoming;

pub use error::CodecError;
pub use incoming::Incoming;

use std::io::{self, Read, Write};

use serde::Serialize;
use serde::de::DeserializeOwned;

/// 命名管道的基名。现在实际用的是带会话号的 `<基名>.<会话号>`（[`crate::instance::session_pipe_name`]）；
/// 不带会话号的这个名字只留给升级前的旧 DLL（[`crate::instance::LEGACY_PIPE_NAME`]）。
pub const DEFAULT_PIPE_NAME: &str = r"\\.\pipe\qingjian";

/// 单帧上限，挡住坏长度前缀导致的巨量分配。
const MAX_FRAME: u32 = 16 * 1024 * 1024;

/// 写一帧：长度前缀 + JSON，然后 flush。
pub fn write_message<W: Write, T: Serialize>(writer: &mut W, value: &T) -> Result<(), CodecError> {
    let body = serde_json::to_vec(value)?;
    let len = u32::try_from(body.len()).map_err(|_| CodecError::TooLarge(body.len()))?;
    if len > MAX_FRAME {
        return Err(CodecError::TooLarge(body.len()));
    }
    writer.write_all(&len.to_le_bytes())?;
    writer.write_all(&body)?;
    writer.flush()?;
    Ok(())
}

/// 读一帧。对端在帧边界干净关闭（读长度前缀时就是 EOF）返回 `Ok(None)`；
/// 读到一半断开算错误（`UnexpectedEof`）。读不懂的消息也算错误——问一句等一答的那一端（DLL）
/// 本来就没法接着往下走；只管收、收不懂可以跳过的那端用 [`read_incoming`]。
pub fn read_message<R: Read, T: DeserializeOwned>(reader: &mut R) -> Result<Option<T>, CodecError> {
    let Some(body) = read_frame(reader)? else {
        return Ok(None);
    };
    Ok(Some(serde_json::from_slice(&body)?))
}

/// 同 [`read_message`]，但把「帧收全了、内容读不懂」单独报成 [`Incoming::Unknown`]：
/// 收的一方跳过这一条接着读下一条，而不是断连接（见 [`Incoming`]）。
pub fn read_incoming<R: Read, T: DeserializeOwned>(
    reader: &mut R,
) -> Result<Incoming<T>, CodecError> {
    let Some(body) = read_frame(reader)? else {
        return Ok(Incoming::Eof);
    };
    match serde_json::from_slice(&body) {
        Ok(message) => Ok(Incoming::Message(message)),
        Err(error) => Ok(Incoming::Unknown(error.to_string())),
    }
}

/// 读一帧的字节。对端在帧边界干净关闭返回 `Ok(None)`。
fn read_frame<R: Read>(reader: &mut R) -> Result<Option<Vec<u8>>, CodecError> {
    let mut len_buf = [0u8; 4];
    if !read_fully_or_eof(reader, &mut len_buf)? {
        return Ok(None);
    }
    let len = u32::from_le_bytes(len_buf);
    if len > MAX_FRAME {
        return Err(CodecError::TooLarge(len as usize));
    }
    let mut body = vec![0u8; len as usize];
    reader.read_exact(&mut body)?;
    Ok(Some(body))
}

/// 填满 `buf`。开头就 EOF（一个字节没读到）返回 `Ok(false)`（干净关闭）；
/// 读到一半 EOF 返回 `UnexpectedEof`；填满返回 `Ok(true)`。
fn read_fully_or_eof<R: Read>(reader: &mut R, buf: &mut [u8]) -> io::Result<bool> {
    let mut filled = 0;
    while filled < buf.len() {
        match reader.read(&mut buf[filled..])? {
            0 if filled == 0 => return Ok(false),
            0 => {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "connection closed mid-frame",
                ));
            }
            read => filled += read,
        }
    }
    Ok(true)
}

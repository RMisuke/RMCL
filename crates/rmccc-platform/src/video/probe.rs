//! 最小 MP4 (`ISO BMFF`) 元数据探测。
//!
//! 只读取 `moov`（不读 `mdat`），解析视频轨的 `tkhd` / `mdhd` / `hdlr` /
//! `stsd` / `stts` / `stsz`。跨平台、无外部依赖，避免在 Windows 上依赖
//! `ffprobe`，也避免把整个视频读进内存。
//!
//! 支持普通 MP4（包括 `moov` 在文件末尾）；fragmented MP4 没有 `stsz`，会
//! 返回 [`RmcccError::Validation`]，由上层向用户显式报错。

use super::{VideoCodec, VideoMetadata};
use rmccc_error::{Result, RmcccError};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

/// 读取 MP4 并返回第一条视频轨的元数据。
pub fn probe_mp4(path: &Path) -> Result<VideoMetadata> {
    let mut file = File::open(path)?;
    let file_len = file.metadata()?.len();
    let moov = read_moov(&mut file, file_len)?;
    parse_moov(&moov)
}

fn read_moov(file: &mut File, file_len: u64) -> Result<Vec<u8>> {
    let mut pos = 0u64;
    while pos + 8 <= file_len {
        file.seek(SeekFrom::Start(pos))?;
        let mut header = [0u8; 8];
        file.read_exact(&mut header)?;
        let mut size = u32::from_be_bytes([header[0], header[1], header[2], header[3]]) as u64;
        let kind = [header[4], header[5], header[6], header[7]];
        let mut header_len = 8u64;
        if size == 1 {
            let mut large = [0u8; 8];
            file.read_exact(&mut large)?;
            size = u64::from_be_bytes(large);
            header_len = 16;
        } else if size == 0 {
            size = file_len - pos;
        }
        if size < header_len || pos + size > file_len {
            return Err(RmcccError::Validation("MP4 box 尺寸越界".into()));
        }
        if &kind == b"moov" {
            // `moov` 通常只有几百 KB；给一个上限，防止损坏文件把内存打满。
            if size - header_len > 128 * 1024 * 1024 {
                return Err(RmcccError::Validation("MP4 moov 过大".into()));
            }
            file.seek(SeekFrom::Start(pos))?;
            let mut data = vec![0u8; size as usize];
            file.read_exact(&mut data)?;
            return Ok(data);
        }
        pos += size;
    }
    Err(RmcccError::Validation("MP4 缺少 moov box".into()))
}

#[derive(Debug, Clone, Copy)]
struct BoxHeader {
    kind: [u8; 4],
    data_start: usize,
    end: usize,
}

fn parse_header(data: &[u8], pos: usize) -> Option<BoxHeader> {
    if pos + 8 > data.len() {
        return None;
    }
    let size32 = u32::from_be_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]]);
    let kind = [data[pos + 4], data[pos + 5], data[pos + 6], data[pos + 7]];
    let (size, header_len) = if size32 == 1 {
        if pos + 16 > data.len() {
            return None;
        }
        let mut large = [0u8; 8];
        large.copy_from_slice(&data[pos + 8..pos + 16]);
        (u64::from_be_bytes(large), 16usize)
    } else if size32 == 0 {
        (data.len().saturating_sub(pos) as u64, 8usize)
    } else {
        (size32 as u64, 8usize)
    };
    let end = pos.checked_add(usize::try_from(size).ok()?)?;
    if size < header_len as u64 || end > data.len() || end < pos + header_len {
        return None;
    }
    Some(BoxHeader {
        kind,
        data_start: pos + header_len,
        end,
    })
}

fn children(data: &[u8], start: usize, end: usize) -> Vec<BoxHeader> {
    let mut result = Vec::new();
    let mut pos = start;
    while let Some(header) = parse_header(data, pos) {
        if header.end > end {
            break;
        }
        result.push(header);
        pos = header.end;
    }
    result
}

fn child(data: &[u8], parent: BoxHeader, kind: &[u8; 4]) -> Option<BoxHeader> {
    children(data, parent.data_start, parent.end)
        .into_iter()
        .find(|header| &header.kind == kind)
}

fn payload(data: &[u8], header: BoxHeader) -> &[u8] {
    &data[header.data_start..header.end]
}

fn be_u32(data: &[u8], offset: usize) -> Option<u32> {
    let bytes = data.get(offset..offset + 4)?;
    Some(u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

fn be_u64(data: &[u8], offset: usize) -> Option<u64> {
    let bytes = data.get(offset..offset + 8)?;
    let mut value = [0u8; 8];
    value.copy_from_slice(bytes);
    Some(u64::from_be_bytes(value))
}

fn parse_moov(moov: &[u8]) -> Result<VideoMetadata> {
    let root =
        parse_header(moov, 0).ok_or_else(|| RmcccError::Validation("MP4 moov 头部损坏".into()))?;
    if &root.kind != b"moov" {
        return Err(RmcccError::Validation("MP4 moov box 类型错误".into()));
    }

    let mut first_error = None;
    for trak in children(moov, root.data_start, root.end) {
        if &trak.kind != b"trak" {
            continue;
        }
        match parse_track(moov, trak) {
            Ok(Some(metadata)) => return Ok(metadata),
            Ok(None) => {}
            Err(error) => first_error = Some(error),
        }
    }
    Err(first_error.unwrap_or_else(|| RmcccError::Validation("MP4 中没有视频轨".into())))
}

fn parse_track(data: &[u8], trak: BoxHeader) -> Result<Option<VideoMetadata>> {
    let Some(tkhd) = child(data, trak, b"tkhd") else {
        return Ok(None);
    };
    let Some(mdia) = child(data, trak, b"mdia") else {
        return Ok(None);
    };
    let Some(hdlr) = child(data, mdia, b"hdlr") else {
        return Ok(None);
    };
    let hdlr_payload = payload(data, hdlr);
    if hdlr_payload.get(8..12) != Some(b"vide") {
        return Ok(None);
    }

    let mut width = 0u32;
    let mut height = 0u32;
    let tkhd_payload = payload(data, tkhd);
    if tkhd_payload.len() >= 8 {
        let matrix_end = tkhd_payload.len() - 8;
        width = be_u32(tkhd_payload, matrix_end).unwrap_or(0) >> 16;
        height = be_u32(tkhd_payload, matrix_end + 4).unwrap_or(0) >> 16;
    }

    let mdhd = child(data, mdia, b"mdhd");
    let (timescale, duration) = mdhd
        .map(|header| parse_mdhd(payload(data, header)))
        .transpose()?
        .unwrap_or((0, 0));

    let Some(stbl) = child(data, mdia, b"minf").and_then(|minf| child(data, minf, b"stbl")) else {
        return Ok(None);
    };

    let codec = child(data, stbl, b"stsd")
        .and_then(|stsd| parse_codec(payload(data, stsd)))
        .unwrap_or_else(|| VideoCodec::Other("unknown".into()));

    if width == 0 || height == 0 {
        if let Some((sample_width, sample_height)) =
            child(data, stbl, b"stsd").and_then(|stsd| parse_sample_size(payload(data, stsd)))
        {
            width = sample_width;
            height = sample_height;
        }
    }

    let (samples, total_delta) = child(data, stbl, b"stts")
        .map(|stts| parse_stts(payload(data, stts)))
        .transpose()?
        .unwrap_or((0, 0));
    let sample_count = child(data, stbl, b"stsz")
        .and_then(|stsz| be_u32(payload(data, stsz), 8).map(u64::from))
        .ok_or_else(|| {
            RmcccError::Validation(
                "MP4 video track has no stsz sample count; fragmented MP4 is unsupported".into(),
            )
        })?;
    if sample_count != samples {
        return Err(RmcccError::Validation(format!(
            "MP4 video sample count mismatch: stts has {samples}, stsz has {sample_count}"
        )));
    }
    let frame_count = sample_count;

    if frame_count == 0 || width == 0 || height == 0 {
        return Ok(None);
    }
    if timescale == 0 {
        return Err(RmcccError::Validation("MP4 视频轨 timescale 为 0".into()));
    }

    let duration_units = if duration > 0 { duration } else { total_delta };
    let duration_ms = if duration_units > 0 {
        duration_units.saturating_mul(1000) / timescale as u64
    } else {
        0
    };

    let (frame_rate_num, frame_rate_den) = if total_delta > 0 {
        reduce_rational(frame_count.saturating_mul(timescale as u64), total_delta)
    } else if duration_ms > 0 {
        reduce_rational(frame_count.saturating_mul(1000), duration_ms)
    } else {
        (0, 1)
    };

    if frame_rate_num == 0 {
        return Err(RmcccError::Validation("MP4 无法计算视频帧率".into()));
    }

    Ok(Some(VideoMetadata {
        width,
        height,
        frame_rate_num,
        frame_rate_den,
        frame_count,
        duration_ms,
        codec,
    }))
}

fn parse_mdhd(data: &[u8]) -> Result<(u32, u64)> {
    let version = data.first().copied().unwrap_or(0);
    if version == 1 {
        let timescale = be_u32(data, 20)
            .ok_or_else(|| RmcccError::Validation("mdhd v1 timescale 缺失".into()))?;
        let duration = be_u64(data, 24)
            .ok_or_else(|| RmcccError::Validation("mdhd v1 duration 缺失".into()))?;
        Ok((timescale, duration))
    } else {
        let timescale =
            be_u32(data, 12).ok_or_else(|| RmcccError::Validation("mdhd timescale 缺失".into()))?;
        let duration = be_u32(data, 16)
            .ok_or_else(|| RmcccError::Validation("mdhd duration 缺失".into()))?
            as u64;
        Ok((timescale, duration))
    }
}

fn parse_stts(data: &[u8]) -> Result<(u64, u64)> {
    let entry_count =
        be_u32(data, 4).ok_or_else(|| RmcccError::Validation("stts 入口数缺失".into()))? as usize;
    let needed = 8usize
        .checked_add(entry_count.saturating_mul(8))
        .ok_or_else(|| RmcccError::Validation("stts 入口数越界".into()))?;
    if data.len() < needed {
        return Err(RmcccError::Validation("stts 数据不完整".into()));
    }
    let mut samples = 0u64;
    let mut total_delta = 0u64;
    for index in 0..entry_count {
        let offset = 8 + index * 8;
        let count = be_u32(data, offset).unwrap_or(0) as u64;
        let delta = be_u32(data, offset + 4).unwrap_or(0) as u64;
        samples = samples.saturating_add(count);
        total_delta = total_delta.saturating_add(count.saturating_mul(delta));
    }
    Ok((samples, total_delta))
}

fn parse_codec(data: &[u8]) -> Option<VideoCodec> {
    let format = data.get(12..16)?;
    Some(match format {
        b"avc1" | b"avc3" => VideoCodec::H264,
        b"hvc1" | b"hev1" => VideoCodec::Hevc,
        other => VideoCodec::Other(String::from_utf8_lossy(other).into_owned()),
    })
}

fn parse_sample_size(data: &[u8]) -> Option<(u32, u32)> {
    // sample entry: size(4) + format(4) + reserved(6) + data_ref(2) +
    // pre_defined(2) + reserved(2) + pre_defined(12) = 32，随后 width/height。
    let width = u16::from_be_bytes(data.get(40..42)?.try_into().ok()?) as u32;
    let height = u16::from_be_bytes(data.get(42..44)?.try_into().ok()?) as u32;
    (width > 0 && height > 0).then_some((width, height))
}

fn reduce_rational(mut numerator: u64, mut denominator: u64) -> (u32, u32) {
    if numerator == 0 || denominator == 0 {
        return (0, 1);
    }
    let divisor = gcd(numerator, denominator);
    numerator /= divisor;
    denominator /= divisor;
    if numerator <= u32::MAX as u64 && denominator <= u32::MAX as u64 {
        (numerator as u32, denominator as u32)
    } else {
        // 极端长视频：用浮点近似，仍然保持可读的帧率。
        let fps = numerator as f64 / denominator as f64;
        (fps.round() as u32, 1)
    }
}

fn gcd(mut a: u64, mut b: u64) -> u64 {
    while b != 0 {
        let remainder = a % b;
        a = b;
        b = remainder;
    }
    a.max(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn boxed(kind: &[u8; 4], payload: &[u8]) -> Vec<u8> {
        let mut result = Vec::with_capacity(8 + payload.len());
        result.extend_from_slice(&((8 + payload.len()) as u32).to_be_bytes());
        result.extend_from_slice(kind);
        result.extend_from_slice(payload);
        result
    }

    fn tkhd(width: u32, height: u32) -> Vec<u8> {
        let mut payload = vec![0u8; 84];
        payload[..4].copy_from_slice(&[0, 0, 0, 7]);
        let offset = payload.len() - 8;
        payload[offset..offset + 4].copy_from_slice(&(width << 16).to_be_bytes());
        payload[offset + 4..offset + 8].copy_from_slice(&(height << 16).to_be_bytes());
        boxed(b"tkhd", &payload)
    }

    fn mdhd(timescale: u32, duration: u32) -> Vec<u8> {
        let mut payload = vec![0u8; 24];
        payload[12..16].copy_from_slice(&timescale.to_be_bytes());
        payload[16..20].copy_from_slice(&duration.to_be_bytes());
        boxed(b"mdhd", &payload)
    }

    fn hdlr() -> Vec<u8> {
        let mut payload = vec![0u8; 24];
        payload[8..12].copy_from_slice(b"vide");
        boxed(b"hdlr", &payload)
    }

    fn stts(sample_count: u32, delta: u32) -> Vec<u8> {
        let mut payload = vec![0u8; 16];
        payload[4..8].copy_from_slice(&1u32.to_be_bytes());
        payload[8..12].copy_from_slice(&sample_count.to_be_bytes());
        payload[12..16].copy_from_slice(&delta.to_be_bytes());
        boxed(b"stts", &payload)
    }

    fn stsz(sample_count: u32) -> Vec<u8> {
        let mut payload = vec![0u8; 12];
        payload[8..12].copy_from_slice(&sample_count.to_be_bytes());
        boxed(b"stsz", &payload)
    }

    fn stsd() -> Vec<u8> {
        let mut entry = vec![0u8; 44];
        entry[4..8].copy_from_slice(b"avc1");
        entry[32..34].copy_from_slice(&1920u16.to_be_bytes());
        entry[34..36].copy_from_slice(&1080u16.to_be_bytes());
        let mut payload = vec![0u8; 8];
        payload[4..8].copy_from_slice(&1u32.to_be_bytes());
        payload.extend_from_slice(&entry);
        boxed(b"stsd", &payload)
    }

    fn video_trak_with_sample_counts(stts_count: u32, stsz_count: u32) -> Vec<u8> {
        let mut stbl_payload = Vec::new();
        stbl_payload.extend_from_slice(&stsd());
        stbl_payload.extend_from_slice(&stts(stts_count, 1_001));
        stbl_payload.extend_from_slice(&stsz(stsz_count));
        let stbl = boxed(b"stbl", &stbl_payload);

        let minf = boxed(b"minf", &stbl);
        let mut mdia_payload = Vec::new();
        mdia_payload.extend_from_slice(&mdhd(30_000, 3_843_840));
        mdia_payload.extend_from_slice(&hdlr());
        mdia_payload.extend_from_slice(&minf);
        let mdia = boxed(b"mdia", &mdia_payload);

        let mut trak_payload = Vec::new();
        trak_payload.extend_from_slice(&tkhd(4_126, 2_596));
        trak_payload.extend_from_slice(&mdia);
        boxed(b"trak", &trak_payload)
    }

    fn video_trak() -> Vec<u8> {
        video_trak_with_sample_counts(3_840, 3_840)
    }

    fn moov() -> Vec<u8> {
        boxed(b"moov", &video_trak())
    }

    #[test]
    fn parses_2997_fps_and_original_frame_count() {
        let metadata = parse_moov(&moov()).expect("moov");
        assert_eq!(metadata.width, 4_126);
        assert_eq!(metadata.height, 2_596);
        assert_eq!(metadata.frame_count, 3_840);
        assert_eq!(metadata.frame_rate_num, 30_000);
        assert_eq!(metadata.frame_rate_den, 1_001);
        assert_eq!(metadata.duration_ms, 128_128);
        assert_eq!(metadata.codec, VideoCodec::H264);
    }

    #[test]
    fn rejects_inconsistent_sample_tables() {
        let trak = video_trak_with_sample_counts(3_840, 3_839);
        let error = parse_moov(&boxed(b"moov", &trak)).expect_err("mismatch must fail");
        assert!(error.to_string().contains("sample count mismatch"));
    }

    #[test]
    fn rejects_missing_moov_header() {
        assert!(parse_moov(b"not an mp4").is_err());
    }

    #[test]
    #[ignore = "需要 RMCCC_TEST_VIDEO 指向一个真实 MP4"]
    fn probes_a_real_mp4_when_requested() {
        let path = std::env::var("RMCCC_TEST_VIDEO").expect("RMCCC_TEST_VIDEO");
        let metadata = probe_mp4(Path::new(&path)).expect("probe");
        assert!(metadata.width > 0);
        assert!(metadata.height > 0);
        assert!(metadata.frame_count > 0);
        assert!(metadata.frame_rate_num > 0);
        eprintln!("{metadata:?}");
    }
}

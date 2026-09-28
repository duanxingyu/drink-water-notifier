use std::io::Cursor;
use std::thread;

const DROP_WAV: &[u8] = include_bytes!("../assets/drop.wav");

/// 在后台播放打包进去的水滴声。没有音频设备时安静跳过。
pub fn play_drop(volume: f32) {
    let volume = volume.clamp(0.0, 1.0);
    if volume <= 0.0 {
        return;
    }
    thread::spawn(move || {
        if let Err(err) = play_blocking(volume) {
            eprintln!("润滴：播放提示音失败：{err}");
        }
    });
}

fn play_blocking(volume: f32) -> Result<(), String> {
    let mut sink = rodio::DeviceSinkBuilder::open_default_sink().map_err(|err| err.to_string())?;
    sink.log_on_drop(false);
    let player = rodio::Player::connect_new(sink.mixer());
    player.set_volume(volume);
    let cursor = Cursor::new(DROP_WAV);
    let decoder = rodio::Decoder::try_from(cursor).map_err(|err| err.to_string())?;
    player.append(decoder);
    player.sleep_until_end();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_drop_is_a_decodable_wav() {
        assert!(DROP_WAV.starts_with(b"RIFF"), "提示音不是 WAV");
        assert!(DROP_WAV.len() > 1000);
        let cursor = Cursor::new(DROP_WAV);
        assert!(rodio::Decoder::try_from(cursor).is_ok());
    }
}

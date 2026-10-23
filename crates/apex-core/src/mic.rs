//! Capture du micro (périphérique d'enregistrement par défaut de Windows), 16 kHz mono 16 bits :
//! le format attendu par la reconnaissance vocale. API winmm `waveIn`, sans dépendance.

pub const SAMPLE_RATE: u32 = 16_000;

#[cfg(windows)]
mod sys {
    use super::SAMPLE_RATE;
    use windows_sys::Win32::Media::Audio::{
        waveInAddBuffer, waveInClose, waveInOpen, waveInPrepareHeader, waveInReset, waveInStart, waveInStop, waveInUnprepareHeader, CALLBACK_NULL, HWAVEIN, WAVEFORMATEX,
        WAVEHDR, WAVE_FORMAT_PCM, WAVE_MAPPER, WHDR_DONE,
    };

    const BUFS: usize = 8;
    /// 100 ms par tampon.
    const BUF_SAMPLES: usize = SAMPLE_RATE as usize / 10;

    pub struct Mic {
        h: HWAVEIN,
        // Tampons et en-têtes à adresse fixe tant que le périphérique est ouvert.
        data: Box<[[i16; BUF_SAMPLES]; BUFS]>,
        hdrs: Box<[WAVEHDR; BUFS]>,
        out: Vec<i16>,
        /// Prochain tampon attendu : les tampons se remplissent dans l'ordre de mise en file.
        next: usize,
    }

    unsafe impl Send for Mic {}

    impl Mic {
        pub fn start() -> Result<Self, String> {
            let fmt = WAVEFORMATEX {
                wFormatTag: WAVE_FORMAT_PCM as u16,
                nChannels: 1,
                nSamplesPerSec: SAMPLE_RATE,
                nAvgBytesPerSec: SAMPLE_RATE * 2,
                nBlockAlign: 2,
                wBitsPerSample: 16,
                cbSize: 0,
            };
            let mut h: HWAVEIN = std::ptr::null_mut();
            let r = unsafe { waveInOpen(&mut h, WAVE_MAPPER, &fmt, 0, 0, CALLBACK_NULL) };
            if r != 0 {
                return Err(format!("micro indisponible (code {r}) : branche un micro ou autorise l'accès au micro dans Windows"));
            }
            let mut m = Mic { h, data: Box::new([[0i16; BUF_SAMPLES]; BUFS]), hdrs: Box::new(unsafe { std::mem::zeroed() }), out: Vec::with_capacity(SAMPLE_RATE as usize * 5), next: 0 };
            for i in 0..BUFS {
                m.hdrs[i].lpData = m.data[i].as_mut_ptr() as *mut u8;
                m.hdrs[i].dwBufferLength = (BUF_SAMPLES * 2) as u32;
                unsafe {
                    waveInPrepareHeader(m.h, &mut m.hdrs[i], std::mem::size_of::<WAVEHDR>() as u32);
                    waveInAddBuffer(m.h, &mut m.hdrs[i], std::mem::size_of::<WAVEHDR>() as u32);
                }
            }
            if unsafe { waveInStart(m.h) } != 0 {
                return Err("démarrage du micro impossible".into());
            }
            Ok(m)
        }

        /// Récupère les tampons remplis, dans l'ordre, et les remet en file (à appeler toutes les ~50 ms).
        pub fn poll(&mut self) {
            for _ in 0..BUFS {
                let i = self.next;
                if self.hdrs[i].dwFlags & WHDR_DONE == 0 {
                    break;
                }
                let n = (self.hdrs[i].dwBytesRecorded as usize / 2).min(BUF_SAMPLES);
                self.out.extend_from_slice(&self.data[i][..n]);
                self.hdrs[i].dwFlags &= !WHDR_DONE;
                self.hdrs[i].dwBytesRecorded = 0;
                unsafe { waveInAddBuffer(self.h, &mut self.hdrs[i], std::mem::size_of::<WAVEHDR>() as u32) };
                self.next = (i + 1) % BUFS;
            }
        }

        /// Arrête et renvoie tout l'audio capturé.
        pub fn stop(mut self) -> Vec<i16> {
            unsafe { waveInStop(self.h) };
            self.poll_final();
            std::mem::take(&mut self.out)
        }

        fn poll_final(&mut self) {
            // waveInStop rend le tampon en cours (partiel) ; waveInReset rend les autres, vides.
            unsafe { waveInReset(self.h) };
            for k in 0..BUFS {
                let i = (self.next + k) % BUFS;
                if self.hdrs[i].dwFlags & WHDR_DONE != 0 && self.hdrs[i].dwBytesRecorded > 0 {
                    let n = (self.hdrs[i].dwBytesRecorded as usize / 2).min(BUF_SAMPLES);
                    self.out.extend_from_slice(&self.data[i][..n]);
                    self.hdrs[i].dwBytesRecorded = 0;
                }
            }
        }
    }

    impl Drop for Mic {
        fn drop(&mut self) {
            unsafe {
                waveInReset(self.h);
                for i in 0..BUFS {
                    waveInUnprepareHeader(self.h, &mut self.hdrs[i], std::mem::size_of::<WAVEHDR>() as u32);
                }
                waveInClose(self.h);
            }
        }
    }
}

#[cfg(not(windows))]
mod sys {
    pub struct Mic;
    impl Mic {
        pub fn start() -> Result<Self, String> {
            Err("micro : Windows uniquement".into())
        }
        pub fn poll(&mut self) {}
        pub fn stop(self) -> Vec<i16> {
            vec![]
        }
    }
}

pub use sys::Mic;

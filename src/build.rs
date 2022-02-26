use alloc::string::String;
use core::fmt::Write;
use core::ops::Deref;
use core::{fmt, mem, ptr, str};

use crate::inline::MAX_INLINE;
use crate::{AFlexStr, FlexStr, IntoAFlexStr, IntoFlexStr, ToAFlexStr, ToFlexStr};

// The size of internal buffer for formatting (if larger needed we punt and just use a heap allocated String)
const BUFFER_SIZE: usize = 1024;

// *** String Buffer ***

// Used to buffer formatting writes before turning into inline string or ref counter string
pub(crate) struct StringBuffer<const N: usize> {
    buffer: [mem::MaybeUninit<u8>; N],
    len: usize,
}

impl<const N: usize> StringBuffer<N> {
    pub fn new() -> Self {
        unsafe {
            // Safety: This should all be ok, because we will never read more then `len` which is
            // never larger than what has been written

            Self {
                buffer: mem::MaybeUninit::uninit().assume_init(),
                len: 0,
            }
        }
    }

    #[inline]
    pub fn capacity(&self) -> usize {
        N
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.len
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Create a new large string buffer copying the existing content
    pub fn to_larger_buffer<const N2: usize>(&self) -> StringBuffer<N2> {
        let mut buffer = StringBuffer::new();

        if !self.is_empty() {
            unsafe {
                // Safety: This should be ok because we only copy what we've already written into
                // a brand new buffer. No way for it to overlap.
                // *** WE DO NEED TO BE CAREFUL TO ENSURE N2 >= self.len ALWAYS ***

                // Copy contents of &str to our data buffer
                ptr::copy_nonoverlapping(
                    self.buffer.as_ptr(),
                    buffer.buffer.as_mut_ptr(),
                    self.len(),
                );
            }
        }

        buffer
    }

    /// Create a new heap allocated string buffer copying the existing content
    pub fn to_string_buffer(&self, cap: usize) -> String {
        let mut buffer = String::with_capacity(cap);

        if !self.is_empty() {
            unsafe {
                // Safety: This should be ok because we only copy what we've already written into
                // a brand new buffer. No way for it to overlap.
                // *** WE DO NEED TO BE CAREFUL TO ENSURE cap >= self.len ALWAYS ***

                // Copy contents of &str to our data buffer
                ptr::copy_nonoverlapping(
                    self.buffer.as_ptr().cast(),
                    buffer.as_mut_ptr(),
                    self.len(),
                );
            }
        }

        buffer
    }

    /// Write the formatting `&str` into the buffer if possible. Returns true if write successful.
    pub fn write(&mut self, s: &str) -> bool {
        let len = self.len();

        if (self.capacity() - len) >= s.len() {
            let buffer = &mut self.buffer[len..];

            unsafe {
                // Safety: we've ensured enough space, moved up position, and no way s can overlap
                ptr::copy_nonoverlapping(s.as_ptr(), buffer.as_mut_ptr().cast(), s.len());
            }
            self.len += s.len();
            true
        } else {
            false
        }
    }
}

impl<const N: usize> Deref for StringBuffer<N> {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        let buffer = &self.buffer[..self.len()];

        unsafe {
            // Safety: The contents are always obtained from a valid UTF8 str, so they must be valid
            // Additionally, we clamp the size of the slice passed to be no longer than our str length
            let buffer = &*(buffer as *const [mem::MaybeUninit<u8>] as *const [u8]);
            str::from_utf8_unchecked(buffer)
        }
    }
}

// *** FlexStr Builder ***

#[allow(clippy::large_enum_variant)]
pub(crate) enum FlexStrBuilder {
    Small(StringBuffer<MAX_INLINE>),
    Regular(StringBuffer<BUFFER_SIZE>),
    Large(String),
}

impl FlexStrBuilder {
    pub fn with_capacity(cap: usize) -> Self {
        if cap <= MAX_INLINE {
            FlexStrBuilder::Small(StringBuffer::new())
        } else if cap <= BUFFER_SIZE {
            FlexStrBuilder::Regular(StringBuffer::new())
        } else {
            FlexStrBuilder::Large(String::with_capacity(cap))
        }
    }

    fn create_string_and_write<const N: usize>(
        buffer: &mut StringBuffer<N>,
        s: &str,
    ) -> FlexStrBuilder {
        let required_cap = buffer.len() + s.len();
        let mut buffer = buffer.to_string_buffer(required_cap * 2);
        // Safety: This always succeeds for String
        unsafe {
            buffer.write_str(s).unwrap_unchecked();
        }
        FlexStrBuilder::Large(buffer)
    }
}

impl Write for FlexStrBuilder {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        match self {
            // TODO: Small probably isn't worth it. Probably makes sense to just keep Regular/Large
            FlexStrBuilder::Small(buffer) => {
                if buffer.write(s) {
                    Ok(())
                } else if s.len() <= BUFFER_SIZE {
                    let mut buffer = buffer.to_larger_buffer();
                    buffer.write(s);
                    *self = FlexStrBuilder::Regular(buffer);
                    Ok(())
                } else {
                    *self = Self::create_string_and_write(buffer, s);
                    Ok(())
                }
            }
            FlexStrBuilder::Regular(buffer) => {
                if buffer.write(s) {
                    Ok(())
                } else {
                    *self = Self::create_string_and_write(buffer, s);
                    Ok(())
                }
            }
            FlexStrBuilder::Large(buffer) => buffer.write_str(s),
        }
    }
}

impl IntoFlexStr for FlexStrBuilder {
    #[inline]
    fn into_flex_str(self) -> FlexStr {
        match self {
            // TODO: If we keep small this can be optimized
            FlexStrBuilder::Small(buffer) => buffer.to_flex_str(),
            FlexStrBuilder::Regular(buffer) => buffer.to_flex_str(),
            FlexStrBuilder::Large(s) => s.into(),
        }
    }
}

impl IntoAFlexStr for FlexStrBuilder {
    #[inline]
    fn into_a_flex_str(self) -> AFlexStr {
        match self {
            // TODO: If we keep small this can be optimized
            FlexStrBuilder::Small(buffer) => buffer.to_a_flex_str(),
            FlexStrBuilder::Regular(buffer) => buffer.to_a_flex_str(),
            FlexStrBuilder::Large(s) => s.into(),
        }
    }
}
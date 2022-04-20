//! Big number types

use std::convert::TryInto; //try_into()

use uint::construct_uint;

pub enum NumConvertError {
    ConversionFailure,
    OtherFailure,
}

// U192
construct_uint! {
    /// 192-bit unsigned integer.
    pub struct U192(3);
}

impl U192 {
    /// Convert u256 to u64
    pub fn to_u64(self) -> Option<u64> {
        self.try_to_u64().map_or_else(|_| None, Some)
    }

    /// Convert u256 to u64
    pub fn try_to_u64(self) -> Result<u64, NumConvertError> {
        self.try_into()
            .map_err(|_| NumConvertError::ConversionFailure)
    }

    /// Convert u256 to u128
    pub fn to_u128(self) -> Option<u128> {
        self.try_to_u128().map_or_else(|_| None, Some)
    }

    /// Convert u256 to u128
    pub fn try_to_u128(self) -> Result<u128, NumConvertError> {
        self.try_into()
            .map_err(|_| NumConvertError::ConversionFailure)
    }
}

// U256
construct_uint! {
    // 256-bit unsigned integer.
    pub struct U256(4);
}

impl U256 {
    /// Convert U256 to u64
    pub fn to_u64(self) -> Option<u64> {
        self.try_to_u64().map_or_else(|_| None, Some)
    }

    /// Convert U256 to u64
    pub fn try_to_u64(self) -> Result<u64, NumConvertError> {
        self.try_into()
            .map_err(|_| NumConvertError::ConversionFailure)
    }

    /// Convert U256 to u128
    pub fn to_u128(self) -> Option<u128> {
        self.try_to_u128().map_or_else(|_| None, Some)
    }

    /// Convert U256 to u128
    pub fn try_to_u128(self) -> Result<u128, NumConvertError> {
        self.try_into()
            .map_err(|_| NumConvertError::ConversionFailure)
    }
}

// U576
construct_uint! {
    /// 576-bit unsigned integer.
    pub struct U576(9);
}

impl U576 {
    /// Convert U576 to u64
    pub fn to_u64(self) -> Option<u64> {
        self.try_to_u64().map_or_else(|_| None, Some)
    }

    /// Convert U576 to u64
    pub fn try_to_u64(self) -> Result<u64, NumConvertError> {
        self.try_into()
            .map_err(|_| NumConvertError::ConversionFailure)
    }

    /// Convert U576 to u128
    pub fn to_u128(self) -> Option<u128> {
        self.try_to_u128().map_or_else(|_| None, Some)
    }

    /// Convert U576 to u128
    pub fn try_to_u128(self) -> Result<u128, NumConvertError> {
        self.try_into()
            .map_err(|_| NumConvertError::ConversionFailure)
    }

    /// Convert U576 to U192
    pub fn to_u192(self) -> Option<U192> {
        self.try_to_u192().map_or_else(|_| None, Some)
    }

    /// Convert U576 to U192
    pub fn try_to_u192(self) -> Result<U192, NumConvertError> {
        self.try_into()
            .map_err(|_| NumConvertError::ConversionFailure)
    }
}
// U704
construct_uint! {
    /// 704-bit unsigned integer.
    pub struct U704(11);
}

impl U704 {
    /// Convert U704 to u64
    pub fn to_u64(self) -> Option<u64> {
        self.try_to_u64().map_or_else(|_| None, Some)
    }

    /// Convert U704 to u64
    pub fn try_to_u64(self) -> Result<u64, NumConvertError> {
        self.try_into()
            .map_err(|_| NumConvertError::ConversionFailure)
    }

    /// Convert U704 to u128
    pub fn to_u128(self) -> Option<u128> {
        self.try_to_u128().map_or_else(|_| None, Some)
    }

    /// Convert U704 to u128
    pub fn try_to_u128(self) -> Result<u128, NumConvertError> {
        self.try_into()
            .map_err(|_| NumConvertError::ConversionFailure)
    }

    /// Convert U704 to U192
    pub fn to_u192(self) -> Option<U192> {
        self.try_to_u192().map_or_else(|_| None, Some)
    }

    /// Convert U704 to U192
    pub fn try_to_u192(self) -> Result<U192, NumConvertError> {
        self.try_into()
            .map_err(|_| NumConvertError::ConversionFailure)
    }
}

/// conversions
impl From<U192> for U576 {
    fn from(value: U192) -> U576 {
        let U192(ref arr) = value;
        let mut ret = [0; 9];
        ret[0] = arr[0];
        ret[1] = arr[1];
        ret[2] = arr[2];
        U576(ret)
    }
}

impl From<U192> for U704 {
    fn from(value: U192) -> U704 {
        let U192(ref arr) = value;
        let mut ret = [0; 11];
        ret[0] = arr[0];
        ret[1] = arr[1];
        ret[2] = arr[2];
        U704(ret)
    }
}

/// conversions
impl From<U576> for U192 {
    fn from(value: U576) -> U192 {
        let U576(ref arr) = value;
        if arr[3] | arr[4] | arr[5] | arr[6] | arr[7] | arr[8] != 0 {
            panic!("Overflow");
        }
        let mut ret = [0; 3];
        ret[0] = arr[0];
        ret[1] = arr[1];
        ret[2] = arr[2];
        U192(ret)
    }
}

impl From<U704> for U192 {
    fn from(value: U704) -> U192 {
        let U704(ref arr) = value;
        if arr[3] | arr[4] | arr[5] | arr[6] | arr[7] | arr[8] | arr[9] | arr[10] != 0 {
            panic!("Overflow");
        }
        let mut ret = [0; 3];
        ret[0] = arr[0];
        ret[1] = arr[1];
        ret[2] = arr[2];
        U192(ret)
    }
}

#[cfg(feature = "std")]
impl str::FromStr for U192 {
    type Err = ::rustc_hex::FromHexError;
    //from hex
    fn from_str(value: &str) -> Result<U192, Self::Err> {
        use rustc_hex::FromHex;

        let bytes: Vec<u8> = match value.len() % 2 == 0 {
            true => r#try!(value.from_hex()),
            false => r#try!(("0".to_owned() + value).from_hex()),
        };

        let bytes_ref: &[u8] = &bytes;
        Ok(From::from(bytes_ref))
    }

    //from decimal
    pub fn from_dec_str(value: &str) -> Result<Self, FromDecStrErr> {
        if !value.bytes().all(|b| b >= 48 && b <= 57) {
            return Err(FromDecStrErr::InvalidCharacter);
        }

        let mut res = Self::default();
        for b in value.bytes().map(|b| b - 48) {
            let (r, overflow) = res.overflowing_mul_u32(10);
            if overflow {
                return Err(FromDecStrErr::InvalidLength);
            }
            let (r, overflow) = r.overflowing_add(b.into());
            if overflow {
                return Err(FromDecStrErr::InvalidLength);
            }
            res = r;
        }
        Ok(res)
    }
}

#[cfg(feature = "std")]
impl str::FromStr for U256 {
    type Err = ::rustc_hex::FromHexError;

    fn from_str(value: &str) -> Result<U256, Self::Err> {
        use rustc_hex::FromHex;

        let bytes: Vec<u8> = match value.len() % 2 == 0 {
            true => r#try!(value.from_hex()),
            false => r#try!(("0".to_owned() + value).from_hex()),
        };

        let bytes_ref: &[u8] = &bytes;
        Ok(From::from(bytes_ref))
    }

    //from decimal
    pub fn from_dec_str(value: &str) -> Result<Self, FromDecStrErr> {
        if !value.bytes().all(|b| b >= 48 && b <= 57) {
            return Err(FromDecStrErr::InvalidCharacter);
        }

        let mut res = Self::default();
        for b in value.bytes().map(|b| b - 48) {
            let (r, overflow) = res.overflowing_mul_u32(10);
            if overflow {
                return Err(FromDecStrErr::InvalidLength);
            }
            let (r, overflow) = r.overflowing_add(b.into());
            if overflow {
                return Err(FromDecStrErr::InvalidLength);
            }
            res = r;
        }
        Ok(res)
    }
}

#[cfg(feature = "std")]
impl str::FromStr for U576 {
    type Err = ::rustc_hex::FromHexError;

    fn from_str(value: &str) -> Result<U576, Self::Err> {
        use rustc_hex::FromHex;

        let bytes: Vec<u8> = match value.len() % 2 == 0 {
            true => r#try!(value.from_hex()),
            false => r#try!(("0".to_owned() + value).from_hex()),
        };

        let bytes_ref: &[u8] = &bytes;
        Ok(From::from(bytes_ref))
    }

    //from decimal
    pub fn from_dec_str(value: &str) -> Result<Self, FromDecStrErr> {
        if !value.bytes().all(|b| b >= 48 && b <= 57) {
            return Err(FromDecStrErr::InvalidCharacter);
        }

        let mut res = Self::default();
        for b in value.bytes().map(|b| b - 48) {
            let (r, overflow) = res.overflowing_mul_u32(10);
            if overflow {
                return Err(FromDecStrErr::InvalidLength);
            }
            let (r, overflow) = r.overflowing_add(b.into());
            if overflow {
                return Err(FromDecStrErr::InvalidLength);
            }
            res = r;
        }
        Ok(res)
    }
}

#[cfg(feature = "std")]
impl str::FromStr for U704 {
    type Err = ::rustc_hex::FromHexError;

    fn from_str(value: &str) -> Result<U704, Self::Err> {
        use rustc_hex::FromHex;

        let bytes: Vec<u8> = match value.len() % 2 == 0 {
            true => r#try!(value.from_hex()),
            false => r#try!(("0".to_owned() + value).from_hex()),
        };

        let bytes_ref: &[u8] = &bytes;
        Ok(From::from(bytes_ref))
    }

    //from decimal
    pub fn from_dec_str(value: &str) -> Result<Self, FromDecStrErr> {
        if !value.bytes().all(|b| b >= 48 && b <= 57) {
            return Err(FromDecStrErr::InvalidCharacter);
        }

        let mut res = Self::default();
        for b in value.bytes().map(|b| b - 48) {
            let (r, overflow) = res.overflowing_mul_u32(10);
            if overflow {
                return Err(FromDecStrErr::InvalidLength);
            }
            let (r, overflow) = r.overflowing_add(b.into());
            if overflow {
                return Err(FromDecStrErr::InvalidLength);
            }
            res = r;
        }
        Ok(res)
    }
}

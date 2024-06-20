use bicycl::b_i_c_y_c_l::Mpz;
use bicycl::cpp_core::CppBox;
use bicycl::cpp_core::MutRef;
use bicycl::cpp_core::Ref;
use bicycl::cpp_std::VectorOfUchar;
use gmp_mpfr_sys::gmp;
use gmp_mpfr_sys::gmp::mpn_sec_div_qr;
use gmp_mpfr_sys::gmp::mpz_cdiv_qr;
use gmp_mpfr_sys::gmp::mpz_limbs_read;
use rand_core::CryptoRng;

use crate::z::EuclideanDivResult;
use crate::z::Z;

/// Signed integer with range (-2^4096, 2^4096)
/// Does not check for overflows!
#[derive(Debug, Clone)]
pub struct Bignum4096 {
    pub positive: bool,
    // we take 128 = 64 * 2 to hold the results of sqr and mul methods
    pub limbs: [u64; 128],
}

fn convert(v: Vec<u8>) -> [u64; 128] {
    assert!(v.len() <= 512);
    let mut array: [u8; 512] = [0; 512];
    array[..v.len()].copy_from_slice(&v);
    let cast: [u64; 64] = bytemuck::cast(array);
    let mut res = [0u64; 128];
    // TODO shouldn't this be 128?
    res[0..64].copy_from_slice(&cast);
    res
}

fn inverse_convert(array: [u64; 128]) -> Vec<u8> {
    let cast: [u64; 64] = array[0..64]
        .try_into()
        .expect("slice with incorrect length");
    let bytes: [u8; 512] = bytemuck::cast(cast);
    let mut res = bytes.to_vec();
    res.reverse();
    res
}

fn mpz_to_bignum(n: &mut CppBox<Mpz>) -> Bignum4096 {
    let mut limbs = unsafe { bicycl::cpp_vec_to_rust(&Mpz::mpz_to_b_i_g_bytes(&mut *n)) };
    limbs.reverse();
    Bignum4096 {
        positive: unsafe { Mpz::sgn(&n) == 1 },
        limbs: convert(limbs),
    }
}

fn bignum_to_mpz(n: &Bignum4096) -> CppBox<Mpz> {
    let limbs = n.limbs.clone();
    //limbs.reverse();
    // TODO we need to specify the sign in the vector of uchar
    let limbs = inverse_convert(limbs);
    let res = unsafe { bicycl::rust_vec_to_cpp(limbs) };
    let res: Ref<VectorOfUchar> = unsafe { Ref::from_raw_ref(&res) };

    let mut r = unsafe { Mpz::new() };
    // let r: MutRef<Mpz> = unsafe { MutRef::from_raw_ref(&mut r) };
    unsafe { Mpz::b_i_g_bytes_to_mpz(&mut r, res) };
    // TODO not constant time!
    if !n.positive {
        unsafe { Mpz::neg(&mut r) }
    }
    // TODO do we need to clear manually the Mpz?
    r
}

impl PartialEq for Bignum4096 {
    fn eq(&self, rhs: &Self) -> bool {
        self.eq_abs(rhs) && self.positive == rhs.positive
    }
}

// Constant-time functions
impl Z for Bignum4096 {
    fn default() -> Self {
        Bignum4096 {
            positive: true,
            limbs: [0; 128],
        }
    }
    fn zero() -> Self {
        Bignum4096 {
            positive: true,
            limbs: [0; 128],
        }
    }

    fn clone(&self) -> Self {
        Clone::clone(&self)
    }

    // returns a positive integer sampled uniformly at random
    fn random<R: CryptoRng>(rng: &mut R) -> Self {
        let mut limbs = [0u64; 128];
        let mut dest = [0u8; 8 * 64];
        rng.fill_bytes(&mut dest);
        for (i, chunk) in dest.chunks_exact(8).enumerate() {
            limbs[i] = u64::from_le_bytes(chunk.try_into().expect("Chunk size is not 8"));
        }
        Bignum4096 {
            positive: true,
            limbs,
        }
    }

    fn eq_abs(&self, rhs: &Self) -> bool {
        let eq: u64 = unsafe {
            hacl_sys::Hacl_Bignum4096_eq_mask(
                self.limbs.as_ptr() as *mut _,
                rhs.limbs.as_ptr() as *mut _,
            )
        };
        eq != 0
    }

    fn eq(&self, rhs: &Self) -> bool {
        self.eq_abs(rhs) && self.positive == rhs.positive
    }

    fn from(n: u64) -> Self {
        let mut res = Self::zero();
        res.limbs[0] = n;
        res
    }

    // TODO: is the inequality strict or not?
    fn less_than_abs(&self, rhs: &Self) -> bool {
        let lt: u64 = unsafe {
            hacl_sys::Hacl_Bignum4096_lt_mask(
                self.limbs.as_ptr() as *mut _,
                rhs.limbs.as_ptr() as *mut _,
            )
        };
        lt != 0
    }

    fn less_than(&self, rhs: &Self) -> bool {
        let both_positive = self.positive && rhs.positive;
        let both_negative = !self.positive && !rhs.positive;
        let self_negative_rhs_positive = !self.positive && rhs.positive;
        let less_than_abs = self.less_than_abs(&rhs);

        let result_if_both_positive = both_positive && less_than_abs;
        let result_if_both_negative = both_negative && !less_than_abs;
        let result_if_self_negative_rhs_positive = self_negative_rhs_positive;

        result_if_both_positive | result_if_both_negative | result_if_self_negative_rhs_positive
    }

    fn add(&self, rhs: &Self) -> Self {
        let mut res = Self::zero();
        // TODO use subtle crate for booleans, or use u32
        match (self.positive, rhs.positive, self.less_than_abs(&rhs)) {
            (true, true, _) => {
                unsafe {
                    hacl_sys::Hacl_Bignum4096_add(
                        self.limbs.as_ptr() as *mut _,
                        rhs.limbs.as_ptr() as *mut _,
                        res.limbs.as_mut_ptr(),
                    );
                }
                res.positive = true;
            }
            (false, false, _) => {
                unsafe {
                    hacl_sys::Hacl_Bignum4096_add(
                        self.limbs.as_ptr() as *mut _,
                        rhs.limbs.as_ptr() as *mut _,
                        res.limbs.as_mut_ptr(),
                    );
                }
                res.positive = false;
            }
            (true, false, true) => {
                unsafe {
                    hacl_sys::Hacl_Bignum4096_sub(
                        rhs.limbs.as_ptr() as *mut _,
                        self.limbs.as_ptr() as *mut _,
                        res.limbs.as_mut_ptr(),
                    );
                }
                res.positive = false;
            }
            (true, false, false) => {
                unsafe {
                    hacl_sys::Hacl_Bignum4096_sub(
                        self.limbs.as_ptr() as *mut _,
                        rhs.limbs.as_ptr() as *mut _,
                        res.limbs.as_mut_ptr(),
                    );
                }
                res.positive = true;
            }
            (false, true, true) => {
                unsafe {
                    hacl_sys::Hacl_Bignum4096_sub(
                        rhs.limbs.as_ptr() as *mut _,
                        self.limbs.as_ptr() as *mut _,
                        res.limbs.as_mut_ptr(),
                    );
                }
                res.positive = true;
            }
            (false, true, false) => {
                unsafe {
                    hacl_sys::Hacl_Bignum4096_sub(
                        self.limbs.as_ptr() as *mut _,
                        rhs.limbs.as_ptr() as *mut _,
                        res.limbs.as_mut_ptr(),
                    );
                }
                res.positive = false;
            }
        }
        res
    }

    fn sub(&self, rhs: &Self) -> Self {
        self.add(&rhs.neg())
    }

    fn mul(&self, rhs: &Self) -> Self {
        let mut res = Self::zero();
        unsafe {
            hacl_sys::Hacl_Bignum4096_mul(
                self.limbs.as_ptr() as *mut _,
                rhs.limbs.as_ptr() as *mut _,
                res.limbs.as_mut_ptr(),
            );
        }
        res.positive = self.positive == rhs.positive;
        res
    }

    fn sqr(&self) -> Self {
        let mut res = Self::zero();
        unsafe {
            hacl_sys::Hacl_Bignum4096_sqr(self.limbs.as_ptr() as *mut _, res.limbs.as_mut_ptr());
        }
        res
    }

    fn neg(&self) -> Self {
        Bignum4096 {
            positive: !self.positive,
            limbs: self.limbs.clone(),
        }
    }

    fn divide_by_2(&mut self) {
        let mut carry: u64 = 0;
        let shift = 63 as u8;
        for limb in self.limbs.iter_mut().rev() {
            let new_carry = *limb & 0b1;
            *limb = (*limb >> 1) | (carry << shift);
            carry = new_carry;
        }
    }

    fn divide_by_4(&mut self) {
        let mut carry: u64 = 0;
        let shift = 62;
        for limb in self.limbs.iter_mut().rev() {
            let new_carry = *limb & 0b11;
            *limb = (*limb >> 2) | (carry << shift);
            carry = new_carry;
        }
    }

    fn is_odd(&self) -> bool {
        self.limbs[0] & 0b1 == 1
    }

    fn oppose(&mut self) {
        self.positive = !self.positive
    }

    fn is_positive(&self) -> bool {
        self.positive
    }

    // make call to mpn_sec_div_qr from gmp
    // use https://docs.rs/gmp-mpfr-sys/latest/i686-unknown-linux-gnu/gmp_mpfr_sys/gmp/fn.mpn_sec_div_qr.html#
    fn euclidean_div_ceil(&self, other: &Self) -> EuclideanDivResult<Self>
    where
        Self: Sized,
    {
        let n = bignum_to_mpz(self);
        let d = bignum_to_mpz(other);
        let mut q = unsafe { Mpz::new() };
        let mut r = unsafe { Mpz::new() };
        let mutref_q: MutRef<Mpz> = unsafe { MutRef::from_raw_ref(&mut q) };
        let mutref_r: MutRef<Mpz> = unsafe { MutRef::from_raw_ref(&mut r) };

        // use gmp_mpfr_sys::gmp::mpn_sec
        unsafe {
            Mpz::cdiv_qr(mutref_q, mutref_r, &n, &d);
        };
        let quotient = mpz_to_bignum(&mut q);
        let remainder = mpz_to_bignum(&mut r);
        EuclideanDivResult {
            quotient,
            remainder,
        }
    }
}

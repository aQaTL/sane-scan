//! API docs: <https://sane-project.gitlab.io/standard/1.06/api.html>

pub mod sys;

pub use sys::*;

use bitflags::bitflags;
use log::{debug, info};
use std::ffi::{c_void, CStr, CString};
use std::fmt::{Debug, Display, Formatter};
use std::ops::Range;

type Result<T> = std::result::Result<T, Error>;

pub struct Error(pub sys::Status);

impl Debug for Error {
	fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
		let status_description =
			unsafe { CStr::from_ptr(sys::sane_strstatus(self.0)).to_string_lossy() };
		f.debug_tuple("Error").field(&status_description).finish()
	}
}

impl Display for Error {
	fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
		let status_description = unsafe { CStr::from_ptr(sys::sane_strstatus(self.0)) };
		write!(f, "{}", status_description.to_string_lossy())
	}
}

impl std::error::Error for Error {}

pub struct Sane {}

impl Drop for Sane {
	fn drop(&mut self) {
		unsafe {
			log::debug!("Drop libsane.");
			sys::sane_exit();
		}
	}
}

impl Sane {
	pub fn init(mut version_code: i32) -> Result<Self> {
		let status = unsafe { sys::sane_init(&mut version_code as *mut sys::Int, None) };
		if status != sys::Status::Good {
			return Err(Error(status));
		}
		Ok(Sane {})
	}

	pub fn init_1_0() -> Result<Self> {
		let build_ver_major = 1;
		let build_ver_minor = 0;
		let build_revision = 0;
		let version_code: i32 =
			build_ver_major << 24 | build_ver_minor << 16 | build_revision & 0xffff;
		Self::init(version_code)
	}

	pub fn get_devices(&self) -> Result<Vec<Device>> {
		let mut device_list: *mut *const sys::Device = std::ptr::null_mut();
		let status: sys::Status =
			unsafe { sys::sane_get_devices(&mut device_list as *mut *mut *const sys::Device, 1) };
		if status != sys::Status::Good {
			return Err(Error(status));
		}
		let device_count = unsafe {
			let mut device_count = 0_usize;
			while !(*device_list.add(device_count)).is_null() {
				device_count += 1;
			}
			device_count
		};
		debug!("Number of devices found: {}.", device_count);
		let device_list: &[*const sys::Device] =
			unsafe { std::slice::from_raw_parts(device_list, device_count) };

		let device_list: Vec<Device> = unsafe {
			device_list
				.iter()
				.copied()
				.map(|device| {
					let name = CStr::from_ptr((*device).name).to_owned();
					let vendor = CStr::from_ptr((*device).vendor).to_owned();
					let model = CStr::from_ptr((*device).model).to_owned();
					let type_ = CStr::from_ptr((*device).type_).to_owned();
					Device {
						name,
						vendor,
						model,
						type_,
					}
				})
				.collect()
		};
		Ok(device_list)
	}
}

#[derive(Debug)]
pub struct Device {
	pub name: CString,
	pub vendor: CString,
	pub model: CString,
	pub type_: CString,
}

impl Device {
	pub fn open(&self) -> Result<DeviceHandle> {
		let mut device_handle: sys::Handle = std::ptr::null_mut();
		let status: sys::Status =
			unsafe { sys::sane_open(self.name.as_ptr(), &mut device_handle as *mut sys::Handle) };
		if status != sys::Status::Good {
			return Err(Error(status));
		}
		Ok(DeviceHandle {
			handle: device_handle,
			scanning: false,
		})
	}
}

pub struct DeviceHandle {
	handle: sys::Handle,
	scanning: bool,
}

impl Drop for DeviceHandle {
	fn drop(&mut self) {
		unsafe {
			if self.scanning {
				sys::sane_cancel(self.handle);
			}
			debug!("Drop DeviceHandle");
			sys::sane_close(self.handle)
		}
	}
}

impl DeviceHandle {
	pub fn get_options(&self) -> Result<Vec<DeviceOption>> {
		let mut option_count_value = 0_i32;
		let status: sys::Status = unsafe {
			sys::sane_control_option(
				self.handle,
				0,
				sys::Action::GetValue,
				(&mut option_count_value as *mut i32).cast::<c_void>(),
				std::ptr::null_mut(),
			)
		};
		if status != sys::Status::Good {
			return Err(Error(status));
		}
		debug!("Number of available options: {}.", option_count_value);

		let mut options = Vec::with_capacity(option_count_value as usize);

		for option_idx in 1..option_count_value {
			let option_descriptor =
				unsafe { sys::sane_get_option_descriptor(self.handle, option_idx) };
			if option_descriptor.is_null() {
				return Err(Error(status));
			}

			unsafe {
				let title = CStr::from_ptr((*option_descriptor).title).to_owned();
				let name = CStr::from_ptr((*option_descriptor).name).to_owned();
				let desc = CStr::from_ptr((*option_descriptor).desc).to_owned();

				let constraint = match (*option_descriptor).constraint_type {
					sys::ConstraintType::None => OptionConstraint::None,
					sys::ConstraintType::Range => {
						let range = (*option_descriptor).constraint.range;
						OptionConstraint::Range {
							range: (*range).min..(*range).max,
							quant: (*range).quant,
						}
					}
					sys::ConstraintType::WordList => {
						let word_list = (*option_descriptor).constraint.word_list;
						let list_len = *word_list;
						let word_list_slice =
							std::slice::from_raw_parts(word_list.add(1), list_len as usize);
						OptionConstraint::WordList(word_list_slice.to_vec())
					}
					sys::ConstraintType::StringList => {
						let string_list = (*option_descriptor).constraint.string_list;
						let mut list_len = 0;
						while !(*(string_list.add(list_len))).is_null() {
							list_len += 1;
						}
						let string_list_vec = std::slice::from_raw_parts(string_list, list_len)
							.iter()
							.map(|&str_ptr| CStr::from_ptr(str_ptr).to_owned())
							.collect::<Vec<_>>();
						OptionConstraint::StringList(string_list_vec)
					}
				};

				options.push(DeviceOption {
					option_idx,
					name,
					title,
					desc,
					type_: (*option_descriptor).type_,
					unit: (*option_descriptor).unit,
					size: (*option_descriptor).size as u32,
					cap: OptionCapability::from_bits_unchecked((*option_descriptor).cap as u32),
					constraint,
				});
			}
		}

		Ok(options)
	}

	pub fn start_scan(&mut self) -> Result<sys::Parameters> {
		let status = unsafe { sys::sane_start(self.handle) };
		if status != sys::Status::Good {
			return Err(Error(status));
		}

		let parameters = self.get_parameters()?;

		self.scanning = true;

		Ok(parameters)
	}

	pub fn get_parameters(&self) -> Result<sys::Parameters> {
		let mut parameters = sys::Parameters::default();

		let status = unsafe {
			sys::sane_get_parameters(self.handle, &mut parameters as *mut sys::Parameters)
		};
		if status != sys::Status::Good {
			return Err(Error(status));
		}
		Ok(parameters)
	}

	pub fn read(&mut self, buf: &mut [u8]) -> Result<Option<usize>> {
		if !self.scanning {
			return Ok(None);
		}
		let mut bytes_written = 0_i32;
		let status = unsafe {
			sys::sane_read(
				self.handle,
				buf.as_mut_ptr(),
				buf.len() as i32,
				&mut bytes_written as *mut i32,
			)
		};
		match status {
			sys::Status::Eof => {
				if bytes_written == 0 {
					self.scanning = false;
					unsafe { sys::sane_cancel(self.handle) };

					Ok(None)
				} else {
					Ok(Some(bytes_written as usize))
				}
			}
			sys::Status::Good => Ok(Some(bytes_written as usize)),
			status => Err(Error(status)),
		}
	}

	pub fn read_to_vec(&mut self) -> Result<Vec<u8>> {
		let parameters = self.get_parameters()?;

		let mut image = Vec::<u8>::with_capacity(
			parameters.bytes_per_line as usize * parameters.lines as usize,
		);

		let mut buf = Vec::<u8>::with_capacity(1024 * 1024);
		unsafe {
			buf.set_len(buf.capacity());
		}

		while let Ok(Some(written)) = self.read(buf.as_mut_slice()) {
			image.extend_from_slice(&buf[0..written]);
		}

		Ok(image)
	}

	pub fn get_option(&self, opt: &DeviceOption) -> Result<DeviceOptionValue> {
		let mut value = vec![0_u8; opt.size as usize];
		let value_ptr = value.as_mut_ptr() as *mut c_void;

		let status = unsafe {
			sys::sane_control_option(
				self.handle,
				opt.option_idx,
				sys::Action::GetValue,
				value_ptr,
				std::ptr::null_mut(),
			)
		};
		if status != sys::Status::Good {
			return Err(Error(status));
		}

		let opt_value = match opt.type_ {
			sys::ValueType::Bool => unsafe { DeviceOptionValue::Bool(*(value_ptr as *const bool)) },
			sys::ValueType::Int => unsafe { DeviceOptionValue::Int(*(value_ptr as *const i32)) },
			sys::ValueType::Fixed => unsafe {
				DeviceOptionValue::Fixed(*(value_ptr as *const i32))
			},
			sys::ValueType::String => unsafe {
				DeviceOptionValue::String(CStr::from_ptr(value_ptr as *const i8).to_owned())
			},
			v @ sys::ValueType::Button | v @ sys::ValueType::Group => {
				panic!("Invalid state. {:?} doesn't represent a value.", v);
			}
		};

		Ok(opt_value)
	}

	pub fn set_option(
		&self,
		opt: &DeviceOption,
		mut value: DeviceOptionValue,
	) -> Result<OptionInfo> {
		let mut str_vec;
		let value_ptr = match value {
			DeviceOptionValue::Bool(ref mut v) => v as *mut bool as *mut i32 as *mut c_void,
			DeviceOptionValue::Int(ref mut v) => v as *mut i32 as *mut c_void,
			DeviceOptionValue::Fixed(ref mut v) => v as *mut i32 as *mut c_void,
			DeviceOptionValue::String(str) => {
				str_vec = str.into_bytes_with_nul();
				str_vec.as_mut_ptr() as *mut c_void
			}
			DeviceOptionValue::Button => std::ptr::null_mut::<c_void>(),
			DeviceOptionValue::Group => {
				panic!("Group option doesn't have a settable value.");
			}
		};

		let mut opt_info = OptionInfo::empty();
		let opt_info_ptr = &mut opt_info as *mut OptionInfo as *mut i32;

		let status = unsafe {
			sys::sane_control_option(
				self.handle,
				opt.option_idx,
				sys::Action::SetValue,
				value_ptr,
				opt_info_ptr,
			)
		};
		if status != sys::Status::Good {
			return Err(Error(status));
		}
		Ok(opt_info)
	}

	pub fn set_option_auto(&self, opt: &DeviceOption) -> Result<OptionInfo> {
		let mut opt_info = OptionInfo::empty();
		let opt_info_ptr = &mut opt_info as *mut OptionInfo as *mut i32;
		let status = unsafe {
			sys::sane_control_option(
				self.handle,
				opt.option_idx,
				sys::Action::SetAuto,
				std::ptr::null_mut(),
				opt_info_ptr,
			)
		};
		if status != sys::Status::Good {
			return Err(Error(status));
		}
		Ok(opt_info)
	}
}

#[derive(Debug)]
pub struct DeviceOption {
	pub option_idx: i32,
	pub name: CString,
	pub title: CString,
	pub desc: CString,
	pub type_: sys::ValueType,
	pub unit: sys::Unit,
	pub size: u32,
	pub cap: OptionCapability,
	pub constraint: OptionConstraint,
}

bitflags! {
	#[repr(transparent)]
	pub struct OptionCapability: u32 {
		const SOFT_SELECT = sys::CAP_SOFT_SELECT;
		const HARD_SELECT = sys::CAP_HARD_SELECT;
		const SOFT_DETECT = sys::CAP_SOFT_DETECT;
		const EMULATED = sys::CAP_EMULATED;
		const AUTOMATIC = sys::CAP_AUTOMATIC;
		const INACTIVE = sys::CAP_INACTIVE;
		const ADVANCED = sys::CAP_ADVANCED;
	}
}

bitflags! {
	#[repr(transparent)]
	pub struct OptionInfo: u32 {
		const INFO_INEXACT = sys::INFO_INEXACT;
		const INFO_RELOAD_OPTIONS = sys::INFO_RELOAD_OPTIONS;
		const INFO_RELOAD_PARAMS = sys::INFO_RELOAD_PARAMS;
	}
}

#[derive(Debug)]
pub enum OptionConstraint {
	None,
	StringList(Vec<CString>),
	WordList(Vec<i32>),
	Range { range: Range<i32>, quant: i32 },
}

#[derive(Debug)]
pub enum DeviceOptionValue {
	Bool(bool),
	Int(i32),
	Fixed(i32),
	String(CString),
	Button,
	Group,
}

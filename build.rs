use bindgen::callbacks::EnumVariantValue;
use convert_case::{Case, Casing};
use std::path::PathBuf;

fn main() {
	let bindings = bindgen::builder()
		.header("/usr/include/sane/sane.h")
		.rustified_enum("SANE_Unit")
		.rustified_enum("SANE_Value_Type")
		.rustified_enum("SANE_Constraint_Type")
		.rustified_enum("SANE_Action")
		.rustified_enum("SANE_Status")
		.rustified_enum("SANE_Frame")
		.prepend_enum_name(false)
		.disable_name_namespacing()
		.disable_nested_struct_naming()
		.derive_debug(true)
		.derive_default(true)
		.parse_callbacks(Box::new(CamelCaseConverterCallback))
		.c_naming(false)
		.generate()
		.unwrap();

	bindings
		.write_to_file(
			[std::env::var("OUT_DIR").unwrap().as_str(), "sane.rs"]
				.iter()
				.collect::<PathBuf>(),
		)
		.unwrap();

	println!("cargo:rustc-link-lib=sane");
}

#[derive(Debug)]
struct CamelCaseConverterCallback;

impl bindgen::callbacks::ParseCallbacks for CamelCaseConverterCallback {
	fn enum_variant_name(
		&self,
		enum_name: Option<&str>,
		original_variant_name: &str,
		_variant_value: EnumVariantValue,
	) -> Option<String> {
		if let Some(mut enum_name) = enum_name {
			if enum_name == "SANE_Value_Type" {
				enum_name = "SANE_TYPE";
			}
			let enum_name = enum_name.strip_suffix("_Type").unwrap_or(&enum_name);
			let enum_name_uppercase = enum_name.to_ascii_uppercase();
			let prefix = format!("{}_", enum_name_uppercase);
			let new_variant_name = original_variant_name
				.strip_prefix(&prefix)
				.unwrap_or(original_variant_name);
			Some(new_variant_name.to_case(Case::UpperCamel))
		} else {
			Some(original_variant_name.to_string())
		}
	}

	fn item_name(&self, original_item_name: &str) -> Option<String> {
		let original_item_name = original_item_name
			.strip_prefix("SANE_")
			.unwrap_or(original_item_name);
		if original_item_name.contains('_')
			&& original_item_name.to_case(Case::Snake) != original_item_name
			&& original_item_name.to_case(Case::UpperSnake) != original_item_name
		{
			return Some(original_item_name.replace('_', ""));
		}
		Some(original_item_name.to_string())
	}
}

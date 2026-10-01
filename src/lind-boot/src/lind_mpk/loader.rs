/// This module contains the minimal loader needed for lind-mpk.
/// It loads our custom ld.so and the application binary into memory.
/// It is not designed to work with any other loader or with binaries that don't require a loader.
/// Uses goblin

use anyhow::{Context, Result, bail};
use cage::{MemoryBackingType, Vmmap, VmmapOps};
use goblin::elf::Elf;
use goblin::elf::header::{ET_DYN, ET_EXEC};
use goblin::elf::program_header::{PF_R, PF_W, PF_X, PT_LOAD, PT_PHDR, ProgramHeader};
use libc::c_ulong;
use std::ffi::c_void;
use std::path::Path;
use sysdefs::constants::fs_const::{PAGESHIFT, PAGESIZE, PROT_EXEC, PROT_READ, PROT_WRITE};

const PAGE_SIZE: usize = PAGESIZE as usize;

#[derive(Debug, Clone, Copy)]
struct LoadedImage {
	load_bias: u64,
	entrypoint: u64,
}

#[derive(Debug, Clone, Copy)]
pub struct LoadedImagesInfo {
	pub ldso_load_bias: u64,
	pub ldso_entrypoint: u64,
	pub binary_load_bias: u64,
	pub binary_entrypoint: u64,
}

pub mod auxv {
	pub const AT_NULL: usize = 0;
	pub const AT_PHDR: usize = 3;
	pub const AT_PHENT: usize = 4;
	pub const AT_PHNUM: usize = 5;
	pub const AT_PAGESZ: usize = 6;
	pub const AT_BASE: usize = 7;
	pub const AT_FLAGS: usize = 8;
	pub const AT_ENTRY: usize = 9;
	pub const AT_UID: usize = 11;
	pub const AT_EUID: usize = 12;
	pub const AT_GID: usize = 13;
	pub const AT_EGID: usize = 14;
	pub const AT_PLATFORM: usize = 15;
	pub const AT_HWCAP: usize = 16;
	pub const AT_CLKTCK: usize = 17;
	pub const AT_SECURE: usize = 23;
	pub const AT_RANDOM: usize = 25;
	pub const AT_HWCAP2: usize = 26;
	pub const AT_EXECFN: usize = 31;
	pub const AT_SYSINFO_EHDR: usize = 33;
	pub const AT_MINSIGSTKSZ: usize = 51;

	// Custom lind auxv entry carrying the syscall interpose trampoline target.
	pub const AT_3ITRMP_PTR: usize = 0x7000_0001;
}

#[derive(Debug, Clone, Copy)]
pub struct ProcessAuxv {
	pub sysinfo_ehdr: usize,
	pub minsigstksz: usize,
	pub hwcap: usize,
	pub pagesz: usize,
	pub clktck: usize,
	pub phdr: usize,
	pub phent: usize,
	pub phnum: usize,
	pub base: usize,
	pub flags: usize,
	pub entry: usize,
	pub uid: usize,
	pub euid: usize,
	pub gid: usize,
	pub egid: usize,
	pub secure: usize,
	pub hwcap2: usize,
	pub threei_trampoline_ptr: usize,
}

fn align_down(value: usize, align: usize) -> usize {
	value & !(align - 1)
}

fn align_up(value: usize, align: usize) -> usize {
	(value + align - 1) & !(align - 1)
}

fn segment_flags_to_prot(flags: u32) -> i32 {
	let mut prot = 0;
	if flags & PF_R != 0 {
		prot |= PROT_READ;
	}
	if flags & PF_W != 0 {
		prot |= PROT_WRITE;
	}
	if flags & PF_X != 0 {
		prot |= PROT_EXEC;
	}
	prot
}

fn validate_elf_for_loader(path: &Path, elf: &Elf) -> Result<()> {
	if !elf.is_64 {
		bail!("loader only supports ELF64: {}", path.display());
	}
	if !elf.little_endian {
		bail!("loader only supports little-endian ELF: {}", path.display());
	}
	if elf.header.e_type != ET_DYN && elf.header.e_type != ET_EXEC {
		bail!(
			"unsupported ELF type {} for {} (expected ET_DYN or ET_EXEC)",
			elf.header.e_type,
			path.display()
		);
	}
	Ok(())
}

fn map_segment(
	vmmap: &mut Vmmap,
	file_bytes: &[u8],
	path: &Path,
	ph: &ProgramHeader,
	load_bias: u64,
) -> Result<()> {
	if ph.p_type != PT_LOAD || ph.p_memsz == 0 {
		return Ok(());
	}

	let seg_vaddr = usize::try_from(ph.p_vaddr)
		.context("segment virtual address does not fit usize")?;
	let seg_file_offset = usize::try_from(ph.p_offset)
		.context("segment file offset does not fit usize")?;
	let seg_file_size = usize::try_from(ph.p_filesz)
		.context("segment file size does not fit usize")?;
	let seg_mem_size = usize::try_from(ph.p_memsz)
		.context("segment memory size does not fit usize")?;

	if seg_file_size > seg_mem_size {
		bail!(
			"invalid PT_LOAD in {}: p_filesz ({}) > p_memsz ({})",
			path.display(),
			seg_file_size,
			seg_mem_size
		);
	}

	let seg_file_end = seg_file_offset
		.checked_add(seg_file_size)
		.context("segment file bounds overflow")?;
	if seg_file_end > file_bytes.len() {
		bail!(
			"invalid PT_LOAD in {}: segment exceeds file size",
			path.display()
		);
	}

	let seg_page_start_vaddr = align_down(seg_vaddr, PAGE_SIZE);
	let seg_page_end_vaddr = align_up(
		seg_vaddr
			.checked_add(seg_mem_size)
			.context("segment address overflow")?,
		PAGE_SIZE,
	);
	let seg_page_len = seg_page_end_vaddr
		.checked_sub(seg_page_start_vaddr)
		.context("segment page range underflow")?;

	let load_bias_usize = usize::try_from(load_bias).context("load bias too large")?;
	let seg_page_start_sys = load_bias_usize
		.checked_add(seg_page_start_vaddr)
		.context("segment system page start overflow")?;
	let seg_data_start_sys = load_bias_usize
		.checked_add(seg_vaddr)
		.context("segment system data start overflow")?;

	let page_num = vmmap.sys_to_page_num(seg_page_start_sys);
	let npages = seg_page_len >> PAGESHIFT;
	let final_prot = segment_flags_to_prot(ph.p_flags);

	vmmap
		.add_entry_with_overwrite(
			page_num,
			npages,
			final_prot,
			final_prot,
			0,
			MemoryBackingType::Anonymous,
			i64::try_from(ph.p_offset).unwrap_or(0),
			i64::try_from(ph.p_filesz).unwrap_or(0),
			0,
		)
		.with_context(|| format!("vmmap add_entry failed for {}", path.display()))?;

	if unsafe {
		libc::mprotect(
			seg_page_start_sys as *mut c_void,
			seg_page_len,
			libc::PROT_READ | libc::PROT_WRITE | libc::PROT_EXEC,
		)
	} != 0
	{
		bail!(
			"mprotect RWX failed while loading {}: {}",
			path.display(),
			std::io::Error::last_os_error()
		);
	}

	unsafe {
		std::ptr::write_bytes(seg_data_start_sys as *mut u8, 0, seg_mem_size);
		std::ptr::copy_nonoverlapping(
			file_bytes[seg_file_offset..seg_file_end].as_ptr(),
			seg_data_start_sys as *mut u8,
			seg_file_size,
		);
	}

	if unsafe { libc::mprotect(seg_page_start_sys as *mut c_void, seg_page_len, final_prot) } != 0 {
		bail!(
			"mprotect final permissions failed while loading {}: {}",
			path.display(),
			std::io::Error::last_os_error()
		);
	}

	Ok(())
}

fn map_elf_into_vmmap(vmmap: &mut Vmmap, path: &Path) -> Result<LoadedImage> {
	let file_bytes = std::fs::read(path)
		.with_context(|| format!("failed to read ELF file {}", path.display()))?;
	let elf = Elf::parse(&file_bytes)
		.with_context(|| format!("failed to parse ELF {}", path.display()))?;
	validate_elf_for_loader(path, &elf)?;

	let mut min_vaddr_page = usize::MAX;
	let mut max_vaddr_page = 0usize;
	let mut found_load_segment = false;

	for ph in &elf.program_headers {
		if ph.p_type != PT_LOAD || ph.p_memsz == 0 {
			continue;
		}

		let seg_vaddr = usize::try_from(ph.p_vaddr)
			.context("segment virtual address does not fit usize")?;
		let seg_mem_size = usize::try_from(ph.p_memsz)
			.context("segment memory size does not fit usize")?;

		let seg_page_start_vaddr = align_down(seg_vaddr, PAGE_SIZE);
		let seg_page_end_vaddr = align_up(
			seg_vaddr
				.checked_add(seg_mem_size)
				.context("segment address overflow")?,
			PAGE_SIZE,
		);

		min_vaddr_page = min_vaddr_page.min(seg_page_start_vaddr);
		max_vaddr_page = max_vaddr_page.max(seg_page_end_vaddr);
		found_load_segment = true;
	}

	if !found_load_segment {
		bail!("no PT_LOAD segments found in {}", path.display());
	}

	let image_span = max_vaddr_page
		.checked_sub(min_vaddr_page)
		.context("invalid PT_LOAD span")?;
	let image_pages = image_span >> PAGESHIFT;
	if image_pages == 0 {
		bail!("computed zero-size PT_LOAD span for {}", path.display());
	}

	let allocation = vmmap
		.find_map_space(image_pages, 1)
		.ok_or_else(|| anyhow::anyhow!("no vmmap space for {}", path.display()))?;
	let image_base_page = allocation.start();
	let image_base_sys = vmmap.page_num_to_sys(image_base_page);
	let load_bias = image_base_sys
		.checked_sub(min_vaddr_page)
		.ok_or_else(|| anyhow::anyhow!("load bias underflow for {}", path.display()))? as u64;

	for ph in &elf.program_headers {
		map_segment(vmmap, &file_bytes, path, ph, load_bias)?;
	}

	let entrypoint = load_bias
		.checked_add(elf.entry)
		.ok_or_else(|| anyhow::anyhow!("entrypoint overflow for {}", path.display()))?;

	Ok(LoadedImage {
		load_bias,
		entrypoint,
	})
}

fn host_auxv_value(kind: usize) -> usize {
	unsafe { libc::getauxval(kind as c_ulong) as usize }
}

fn host_sysconf_value(name: i32, fallback: usize) -> usize {
	let value = unsafe { libc::sysconf(name) };
	if value > 0 {
		value as usize
	}
	else {
		fallback
	}
}

fn compute_phdr_addr(elf: &Elf, load_bias: u64) -> Option<u64> {
	if let Some(phdr_seg) = elf.program_headers.iter().find(|ph| ph.p_type == PT_PHDR) {
		return load_bias.checked_add(phdr_seg.p_vaddr);
	}

	let phdr_file_off = elf.header.e_phoff;
	let phdr_table_len =
		(elf.header.e_phentsize as u64).checked_mul(elf.header.e_phnum as u64)?;
	let phdr_file_end = phdr_file_off.checked_add(phdr_table_len)?;

	for ph in &elf.program_headers {
		if ph.p_type != PT_LOAD {
			continue;
		}

		let seg_file_start = ph.p_offset;
		let seg_file_end = ph.p_offset.checked_add(ph.p_filesz)?;
		if phdr_file_off >= seg_file_start && phdr_file_end <= seg_file_end {
			let delta = phdr_file_off.checked_sub(seg_file_start)?;
			let phdr_vaddr = ph.p_vaddr.checked_add(delta)?;
			return load_bias.checked_add(phdr_vaddr);
		}
	}

	load_bias.checked_add(phdr_file_off)
}

pub fn build_process_auxv(
	binary_path: &str,
	loaded: &LoadedImagesInfo,
	threei_trampoline_ptr: usize,
) -> Result<ProcessAuxv> {
	let file_bytes = std::fs::read(binary_path)
		.with_context(|| format!("failed to read ELF file {}", binary_path))?;
	let elf = Elf::parse(&file_bytes)
		.with_context(|| format!("failed to parse ELF file {}", binary_path))?;

	let phdr = compute_phdr_addr(&elf, loaded.binary_load_bias)
		.context("failed to compute AT_PHDR address")?;

	Ok(ProcessAuxv {
		sysinfo_ehdr: host_auxv_value(auxv::AT_SYSINFO_EHDR),
		minsigstksz: host_auxv_value(auxv::AT_MINSIGSTKSZ),
		hwcap: host_auxv_value(auxv::AT_HWCAP),
		pagesz: host_sysconf_value(libc::_SC_PAGESIZE, 4096),
		clktck: host_sysconf_value(libc::_SC_CLK_TCK, 100),
		phdr: phdr as usize,
		phent: elf.header.e_phentsize as usize,
		phnum: elf.header.e_phnum as usize,
		base: loaded.ldso_load_bias as usize,
		flags: 0,
		entry: loaded.binary_entrypoint as usize,
		uid: unsafe { libc::getuid() as usize },
		euid: unsafe { libc::geteuid() as usize },
		gid: unsafe { libc::getgid() as usize },
		egid: unsafe { libc::getegid() as usize },
		secure: host_auxv_value(auxv::AT_SECURE),
		hwcap2: host_auxv_value(auxv::AT_HWCAP2),
		threei_trampoline_ptr,
	})
}



//loads the custom ld.so and the application binary into memory
//returns the entry point of ld.so
pub fn mpk_load_ldso_and_binary_inplace(ldso_path: &str, binary_path: &str, vmmap: &mut Vmmap) -> Result<u64> {
	if vmmap.base_address.is_none() {
		bail!("vmmap base_address must be initialized before loading ELF images");
	}

	let ldso = map_elf_into_vmmap(vmmap, Path::new(ldso_path))?;
	let _binary = map_elf_into_vmmap(vmmap, Path::new(binary_path))?;
	Ok(ldso.entrypoint)
}

pub fn mpk_load_ldso_and_binary_with_info(
	ldso_path: &str,
	binary_path: &str,
	vmmap: &mut Vmmap,
) -> Result<LoadedImagesInfo> {
	if vmmap.base_address.is_none() {
		bail!("vmmap base_address must be initialized before loading ELF images");
	}

	let ldso = map_elf_into_vmmap(vmmap, Path::new(ldso_path))?;
	let binary = map_elf_into_vmmap(vmmap, Path::new(binary_path))?;

	Ok(LoadedImagesInfo {
		ldso_load_bias: ldso.load_bias,
		ldso_entrypoint: ldso.entrypoint,
		binary_load_bias: binary.load_bias,
		binary_entrypoint: binary.entrypoint,
	})
}


#[cfg(test)]
mod tests {
	use super::*;
	use std::path::{Path, PathBuf};
	use std::process::Command;

	#[derive(Debug, Clone, Copy, PartialEq, Eq)]
	struct LoadSeg {
		offset: u64,
		vaddr: u64,
		filesz: u64,
		memsz: u64,
		flags: u32,
	}

	fn parse_hex_u64(token: &str) -> u64 {
		u64::from_str_radix(token.trim_start_matches("0x"), 16)
			.expect("failed to parse hex field")
	}

	fn parse_flag_letters(flag_letters: &str) -> u32 {
		let mut flags = 0;
		if flag_letters.contains('R') {
			flags |= PF_R;
		}
		if flag_letters.contains('W') {
			flags |= PF_W;
		}
		if flag_letters.contains('E') {
			flags |= PF_X;
		}
		flags
	}

	fn real_ldso_path() -> PathBuf {
		let path = Path::new(env!("CARGO_MANIFEST_DIR"))
			.join("..")
			.join("..")
			.join("lindfs")
			.join("lib")
			.join("ld.so");
		assert!(path.is_file(), "missing test ELF file: {}", path.display());
		path
	}

	fn real_libc_path() -> PathBuf {
		let path = Path::new(env!("CARGO_MANIFEST_DIR"))
			.join("..")
			.join("..")
			.join("lindfs")
			.join("lib")
			.join("elf")
			.join("libc.so");
		assert!(path.is_file(), "missing test ELF file: {}", path.display());
		path
	}

	fn goblin_load_segments(path: &Path) -> Vec<LoadSeg> {
		let file_bytes = std::fs::read(path).expect("failed to read ELF file for test");
		let elf = Elf::parse(&file_bytes).expect("failed to parse ELF file with goblin");
		elf.program_headers
			.iter()
			.filter(|ph| ph.p_type == PT_LOAD && ph.p_memsz > 0)
			.map(|ph| LoadSeg {
				offset: ph.p_offset,
				vaddr: ph.p_vaddr,
				filesz: ph.p_filesz,
				memsz: ph.p_memsz,
				flags: ph.p_flags,
			})
			.collect()
	}

	fn readelf_load_segments(path: &Path) -> Vec<LoadSeg> {
		let output = Command::new("readelf")
			.arg("-Wl")
			.arg(path)
			.output()
			.expect("failed to execute readelf");
		assert!(
			output.status.success(),
			"readelf failed for {}: {}",
			path.display(),
			String::from_utf8_lossy(&output.stderr)
		);

		let stdout = String::from_utf8(output.stdout).expect("readelf output was not valid UTF-8");
		stdout
			.lines()
			.filter_map(|line| {
				let fields: Vec<&str> = line.split_whitespace().collect();
				if fields.first().copied() != Some("LOAD") {
					return None;
				}
				assert!(fields.len() >= 8, "unexpected readelf LOAD line: {line}");
				let flags = fields[6..fields.len() - 1].join("");
				Some(LoadSeg {
					offset: parse_hex_u64(fields[1]),
					vaddr: parse_hex_u64(fields[2]),
					filesz: parse_hex_u64(fields[4]),
					memsz: parse_hex_u64(fields[5]),
					flags: parse_flag_letters(&flags),
				})
			})
			.collect()
	}

	fn infer_load_bias_from_vmmap(vmmap: &Vmmap, segments: &[LoadSeg]) -> Option<usize> {
		for seg in segments {
			let seg_vaddr = usize::try_from(seg.vaddr).ok()?;
			let seg_page_vaddr = align_down(seg_vaddr, PAGE_SIZE);
			let expected_prot = segment_flags_to_prot(seg.flags);

			for (_, entry_desc) in vmmap.double_ended_iter() {
				if entry_desc.file_offset != seg.offset as i64
					|| entry_desc.file_size != seg.filesz as i64
					|| entry_desc.prot != expected_prot
				{
					continue;
				}

				let mapped_page_sys = vmmap.page_num_to_sys(entry_desc.page_num);
				if mapped_page_sys < seg_page_vaddr {
					continue;
				}
				let candidate_load_bias = mapped_page_sys - seg_page_vaddr;

				let all_match = segments.iter().all(|expected| {
					let expected_vaddr = match usize::try_from(expected.vaddr) {
						Ok(v) => v,
						Err(_) => return false,
					};
					let expected_page_sys = candidate_load_bias + align_down(expected_vaddr, PAGE_SIZE);
					let expected_page = vmmap.sys_to_page_num(expected_page_sys);
					let Some(found) = vmmap.find_page(expected_page) else {
						return false;
					};
					found.file_offset == expected.offset as i64
						&& found.file_size == expected.filesz as i64
						&& found.prot == segment_flags_to_prot(expected.flags)
				});

				if all_match {
					return Some(candidate_load_bias);
				}
			}
		}

		None
	}

	fn make_test_vmmap(mapped_size: usize) -> (Vmmap, *mut c_void) {
		let base = unsafe {
			libc::mmap(
				std::ptr::null_mut(),
				mapped_size,
				libc::PROT_READ | libc::PROT_WRITE,
				libc::MAP_PRIVATE | libc::MAP_ANONYMOUS,
				-1,
				0,
			)
		};
		assert_ne!(base, libc::MAP_FAILED);

		let mut vmmap = Vmmap::new();
		vmmap.set_base_address(base as usize);
		vmmap.user_addr_bit_width = cage::VmmapBitWidth::Vmmap64Bit;
		vmmap.start_address = 0;
		vmmap.end_address = mapped_size >> PAGESHIFT;
		(vmmap, base)
	}

	#[test]
	fn readelf_and_goblin_agree_on_ldso_load_segments() {
		let ldso = real_ldso_path();
		let goblin = goblin_load_segments(&ldso);
		let readelf = readelf_load_segments(&ldso);

		assert!(!goblin.is_empty(), "goblin found no PT_LOAD segments");
		assert_eq!(
			goblin.len(),
			readelf.len(),
			"goblin/readelf disagree on PT_LOAD count"
		);
		for (idx, (lhs, rhs)) in goblin.iter().zip(readelf.iter()).enumerate() {
			assert_eq!(lhs, rhs, "LOAD segment mismatch at index {idx}");
		}
	}

	#[test]
	fn maps_real_ldso_segments_with_correct_offsets_and_permissions() {
		let ldso = real_ldso_path();
		let file_bytes = std::fs::read(&ldso).expect("failed to read ld.so bytes");
		let expected_segments = goblin_load_segments(&ldso);

		let mapped_size = PAGE_SIZE * 1024;
		let (mut vmmap, base) = make_test_vmmap(mapped_size);

		let image = map_elf_into_vmmap(&mut vmmap, &ldso).expect("failed to map real ld.so");
		let parsed = Elf::parse(&file_bytes).expect("failed to parse ld.so for entrypoint check");
		assert_eq!(image.entrypoint, image.load_bias + parsed.entry);

		for seg in expected_segments {
			let seg_vaddr = usize::try_from(seg.vaddr).expect("segment vaddr too large");
			let seg_offset = usize::try_from(seg.offset).expect("segment file offset too large");
			let seg_filesz = usize::try_from(seg.filesz).expect("segment filesz too large");
			let seg_memsz = usize::try_from(seg.memsz).expect("segment memsz too large");

			let seg_page_sys = (image.load_bias as usize) + align_down(seg_vaddr, PAGE_SIZE);
			let mapped_page = vmmap.sys_to_page_num(seg_page_sys);
			let vmmap_entry = vmmap
				.find_page(mapped_page)
				.expect("segment page missing in vmmap");

			assert_eq!(
				vmmap_entry.prot,
				segment_flags_to_prot(seg.flags),
				"unexpected vmmap protection for segment at vaddr=0x{:x}",
				seg.vaddr
			);
			assert_eq!(vmmap_entry.file_offset, seg.offset as i64);
			assert_eq!(vmmap_entry.file_size, seg.filesz as i64);

			if seg_filesz > 0 {
				let sample_len = seg_filesz.min(64);
				let sample_mem = unsafe {
					std::slice::from_raw_parts(
						((image.load_bias as usize) + seg_vaddr) as *const u8,
						sample_len,
					)
				};
				assert_eq!(
					sample_mem,
					&file_bytes[seg_offset..seg_offset + sample_len],
					"file contents were not copied correctly for vaddr=0x{:x}",
					seg.vaddr
				);
			}

			if seg_memsz > seg_filesz {
				let zero_len = (seg_memsz - seg_filesz).min(64);
				let bss_sample = unsafe {
					std::slice::from_raw_parts(
						((image.load_bias as usize) + seg_vaddr + seg_filesz) as *const u8,
						zero_len,
					)
				};
				assert!(
					bss_sample.iter().all(|b| *b == 0),
					"BSS tail is not zeroed for vaddr=0x{:x}",
					seg.vaddr
				);
			}
		}

		unsafe {
			libc::munmap(base, mapped_size);
		}
	}

	#[test]
	fn loader_returns_real_ldso_entrypoint_and_maps_two_real_images() {
		let ldso_path = real_ldso_path();
		let libc_path = real_libc_path();

		let ldso_bytes = std::fs::read(&ldso_path).expect("failed to read ld.so");
		let ldso_load_segments = goblin_load_segments(&ldso_path);
		let libc_load_segments = goblin_load_segments(&libc_path);
		let libc_segment_count = libc_load_segments.len();

		let mapped_size = PAGE_SIZE * 2048;
		let (mut vmmap, base) = make_test_vmmap(mapped_size);

		let entry = mpk_load_ldso_and_binary_inplace(
			ldso_path.to_str().expect("invalid UTF-8 path"),
			libc_path.to_str().expect("invalid UTF-8 path"),
			&mut vmmap,
		)
		.expect("loader failed for real ld.so/libc.so");

		let ldso_elf = Elf::parse(&ldso_bytes).expect("failed to parse ld.so");
		let inferred_ldso_bias = infer_load_bias_from_vmmap(&vmmap, &ldso_load_segments)
			.expect("failed to infer ld.so load bias from vmmap entries");

		let mut found_libc_mappings = 0usize;
		for seg in libc_load_segments {
			let expected_prot = segment_flags_to_prot(seg.flags);
			let mut matched = false;
			for (_, entry_desc) in vmmap.double_ended_iter() {
				if entry_desc.file_offset == seg.offset as i64
					&& entry_desc.file_size == seg.filesz as i64
					&& entry_desc.prot == expected_prot
				{
					matched = true;
					break;
				}
			}
			if matched {
				found_libc_mappings += 1;
			}
		}

		assert_eq!(
			entry,
			(inferred_ldso_bias as u64) + ldso_elf.entry,
			"returned entrypoint did not match inferred ld.so load bias + e_entry"
		);
		assert!(
			found_libc_mappings == libc_segment_count,
			"not all libc LOAD segments were reflected in vmmap"
		);

		unsafe {
			libc::munmap(base, mapped_size);
		}
	}
}
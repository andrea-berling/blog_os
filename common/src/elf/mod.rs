// https://refspecs.linuxfoundation.org/elf/gabi4+/ch4.eheader.html#elfid

pub mod header;
pub mod program_header;
pub mod section;

use crate::error::{self, Context, Error, Facility, Fault};

pub struct File<'a> {
    bytes: &'a [u8],
    header: header::Header,
}

impl<'a> File<'a> {
    /// # Panics
    /// Will panic if the size of the ELF file was not validated to contain enough bytes for the
    /// section header, and if that state wasn't preserved
    pub fn sections(&self) -> section::SectionHeaderEntries<'a> {
        let n_entries = self.header.section_header_entries();

        section::SectionHeaderEntries::new(
            &self.bytes[self.header.section_header_offset() as usize..]
                [..(self.header.section_header_entry_size() * n_entries) as usize],
            self.header.class(),
            n_entries,
        )
        .expect("not enough bytes for the section header")
    }

    /// # Panics
    /// Will panic if the size of the ELF file was not validated to contain enough bytes for the
    /// program header, and if that state wasn't preserved
    pub fn program_headers(&self) -> program_header::ProgramHeaderEntries<'a> {
        let n_entries = self.header.program_header_entries();

        program_header::ProgramHeaderEntries::new(
            &self.bytes[self.header.program_header_offset() as usize..]
                [..(self.header.program_header_entry_size() * n_entries) as usize],
            self.header.class(),
            n_entries,
        )
        .expect("not enough bytes for the program header")
    }

    pub fn get_section_by_index(
        &self,
        index: usize,
    ) -> Option<error::Result<section::Section<'_>>> {
        if index >= self.header.section_header_entries() as usize {
            return None;
        }

        match section::HeaderEntry::try_from_bytes(
            self.bytes.get(
                (self.header.section_header_offset() as usize
                    + index * self.header.section_header_entry_size() as usize)..,
            )?,
            self.header.class(),
        ) {
            Ok(section_entry_header) => {
                let offset = section_entry_header.offset() as usize;
                Some(
                    section_entry_header.try_to_entry(
                        self.bytes
                            .get(offset..offset + section_entry_header.size() as usize)?,
                    ),
                )
            }
            Err(err) => Some(Err(
                err.with_facility(Facility::ElfSectionHeaderEntry(index as u16))
            )),
        }
    }

    pub fn get_segment(&self, program_header: &program_header::HeaderEntry) -> Option<&[u8]> {
        self.bytes.get(
            (program_header.offset() as usize)
                ..(program_header.offset() + program_header.segment_size_on_file()) as usize,
        )
    }

    pub fn header(&self) -> &header::Header {
        &self.header
    }
}

impl<'a> TryFrom<&'a [u8]> for File<'a> {
    type Error = Error;

    fn try_from(bytes: &'a [u8]) -> error::Result<Self> {
        let error = Error::blank()
            .with_context(Context::Parsing)
            .with_facility(Facility::ElfFile);

        let result = Self {
            bytes,
            header: bytes.try_into()?,
        };

        if result.bytes.len() < result.header.section_header_offset() as usize
            || result.bytes.len()
                < (result.header.section_header_offset()
                    + (result.header.section_header_entry_size()
                        * result.header.section_header_entries()) as u64) as usize
        {
            return Err(error.with_fault(Fault::NotEnoughBytesFor("section header")));
        }

        if result.bytes.len() < result.header.program_header_offset() as usize
            || result.bytes.len()
                < (result.header.program_header_offset()
                    + (result.header.program_header_entry_size()
                        * result.header.program_header_entries()) as u64) as usize
        {
            return Err(error.with_fault(Fault::NotEnoughBytesFor("program header")));
        }

        Ok(Self {
            bytes,
            header: bytes.try_into()?,
        })
    }
}

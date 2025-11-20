// https://refspecs.linuxfoundation.org/elf/gabi4+/ch4.eheader.html#elfid

pub mod header;
pub mod program_header;
pub mod section;

use crate::error::Error;
use crate::error::Facility;
use crate::error::Fault;

type Halfword = u16;
type Word = u32;

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
    ) -> Option<Result<section::Section<'_>, Error>> {
        if index >= self.header.section_header_entries() as usize {
            return None;
        }

        let error_reporting_facility = Facility::ElfSectionHeaderEntry(index as Halfword);

        match section::HeaderEntry::try_from_bytes(
            self.bytes.get(
                (self.header.section_header_offset() as usize
                    + index * self.header.section_header_entry_size() as usize)..,
            )?,
            self.header.class(),
            error_reporting_facility,
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
            Err(err) => Some(Err(err)),
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

    fn try_from(bytes: &'a [u8]) -> core::result::Result<Self, Self::Error> {
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
            return Err(Error::parsing_error(
                Fault::NotEnoughBytesFor("section header"),
                Facility::ElfFile,
            ));
        }

        if result.bytes.len() < result.header.program_header_offset() as usize
            || result.bytes.len()
                < (result.header.program_header_offset()
                    + (result.header.program_header_entry_size()
                        * result.header.program_header_entries()) as u64) as usize
        {
            return Err(Error::parsing_error(
                Fault::NotEnoughBytesFor("program header"),
                Facility::ElfFile,
            ));
        }

        Ok(Self {
            bytes,
            header: bytes.try_into()?,
        })
    }
}

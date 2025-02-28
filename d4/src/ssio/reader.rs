use std::io::{Error, ErrorKind, Read, Result, Seek};

use d4_framefile::{Blob, Directory, OpenResult};

use crate::{Chrom, Header, d4file::validate_header, index::{D4IndexCollection, DataIndexRef, DataSummary, SecondaryFrameIndex}, ptab::PRIMARY_TABLE_NAME, stab::{CompressionMethod, RecordBlockParsingState, SECONDARY_TABLE_NAME}};

use super::{table::SecondaryTableRef, view::D4TrackView};

pub struct D4TrackReader<R: Read + Seek> {
    header: Header,
    primary_table: Blob<R>,
    secondary_table: Vec<SecondaryTableRef<R>>,
    sfi: Option<SecondaryFrameIndex>,
    track_root: Directory<R>,
}

impl<R: Read + Seek> D4TrackReader<R> {
    pub fn chrom_list(&self) -> &[Chrom] {
        self.header.chrom_list()
    }
    pub fn get_view(&mut self, chrom: &str, begin: u32, end: u32) -> Result<D4TrackView<R>> {
        let primary_offset = self.header.primary_table_offset_of_chrom(chrom);
        let primary_size = self.header.primary_table_size_of_chrom(chrom);
        if primary_size == 0 && self.header.dictionary().bit_width() != 0{
            return Err(Error::new(ErrorKind::Other, "chrom name not found"));
        }

        let primary_view = self
            .primary_table
            .get_view(primary_offset as u64, primary_size);

        let mut secondary_view = Vec::new();

        let chrom_id = self.header.get_chrom_id(chrom).unwrap();

        for table_ref in self.secondary_table.iter() {
            if table_ref.chrom_id == chrom_id {
                let overlap_begin = table_ref.begin.max(begin);
                let overlap_end = table_ref.end.min(end);
                if overlap_begin < overlap_end {
                    if overlap_begin == table_ref.begin || self.sfi.is_none() {
                        secondary_view.push(table_ref.clone());
                    } else {
                        let sfi = self.sfi.as_ref().unwrap();
                        if let Some(addr) = sfi.find_partial_seconary_table(chrom, overlap_begin)? {
                            secondary_view.push(SecondaryTableRef::new_partial(
                                table_ref.root.clone(),
                                chrom_id,
                                overlap_begin,
                                overlap_end,
                                addr,
                                table_ref.comp,
                            ));
                        }
                    }
                }
            }
        }
        Ok(D4TrackView {
            fetch_size: 65536.min(primary_view.size()),
            primary_table: primary_view,
            secondary_tables: secondary_view.into(),
            chrom: chrom.to_string(),
            end,
            cursor: begin,
            primary_table_buffer: None,
            dictionary: self.header.dictionary().clone(),
            stream: None,
            current_record: None,
            rbp_state: RecordBlockParsingState::new(CompressionMethod::NoCompression),
            frame_decode_result: Default::default(),
        })
    }

    pub fn from_track_root(track_root: Directory<R>) -> Result<Self> {
        let header_stream = track_root.open_stream(Header::HEADER_STREAM_NAME)?;
        let header = Header::read(header_stream)?;
        let primary_table = track_root.open_blob(PRIMARY_TABLE_NAME)?;

        let secondary_table = SecondaryTableRef::create_stream_index(
            track_root.open_directory(SECONDARY_TABLE_NAME)?,
            header.chrom_list(),
        )?;

        let sfi = D4IndexCollection::from_root_container(&track_root)
            .and_then(|ic| ic.load_seconary_frame_index())
            .ok();

        Ok(D4TrackReader {
            header,
            primary_table,
            secondary_table,
            sfi,
            track_root,
        })
    }

    pub fn load_data_index<S: DataSummary>(&self) -> Result<DataIndexRef<S>> {
        let ic = D4IndexCollection::from_root_container(&self.track_root)?;
        ic.load_data_index::<S>()
    }

    pub fn from_reader(mut reader: R, track_name: Option<&str>) -> Result<Self> {
        validate_header(&mut reader)?;
        let file_root = Directory::open_root(reader, 8)?;
        let track_root = if let Some(track_name) = track_name {
            match file_root.open(track_name)? {
                OpenResult::SubDir(root) => root,
                _ => {
                    return Err(Error::new(
                        std::io::ErrorKind::Other,
                        "track root not found",
                    ));
                }
            }
        } else {
            file_root
        };
        Self::from_track_root(track_root)
    }
}

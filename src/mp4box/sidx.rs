use byteorder::{BigEndian, ReadBytesExt};
use serde::Serialize;
use std::io::{self, Read, Seek, SeekFrom};

use crate::mp4box::*;

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize)]
pub struct SidxBox {
    pub version: u8,
    pub flags: u32,
    pub reference_id: u32,
    pub timescale: u32,
    pub earliest_presentation_time: u64,
    pub first_offset: u64,
    pub references: Vec<SidxReference>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize)]
pub struct SidxReference {
    pub reference_type: u8,
    pub referenced_size: u32,
    pub subsegment_duration: u32,
    pub starts_with_sap: u8,
    pub sap_type: u8,
    pub sap_delta_time: u32,
}

impl SidxBox {
    pub fn get_type(&self) -> BoxType {
        BoxType::SidxBox
    }
    pub fn get_size(&self) -> u64 {
        HEADER_SIZE
            + (4 + (if self.version == 1 { 2 } else { 1 }) * 2) * 4
            + (12 * self.references.len() as u64)
    }
}

impl Mp4Box for SidxBox {
    fn box_type(&self) -> BoxType {
        self.get_type()
    }

    fn box_size(&self) -> u64 {
        self.get_size()
    }

    fn to_json(&self) -> Result<String> {
        Ok(serde_json::to_string(&self).unwrap())
    }

    fn summary(&self) -> Result<String> {
        todo!()
    }
}

impl<R: Read + Seek> ReadBox<&mut R> for SidxBox {
    fn read_box(reader: &mut R, size: u64) -> Result<Self> {
        let start = box_start(reader)?;

        let version = reader.read_u8()?;
        let flags = reader.read_u24::<BigEndian>()?; //unused flags

        let reference_id = reader.read_u32::<BigEndian>()?;
        let timescale = reader.read_u32::<BigEndian>()?;

        let earliest_presentation_time = if version == 1 {
            reader.read_u64::<BigEndian>()?
        } else {
            reader.read_u32::<BigEndian>()? as u64
        };

        let first_offset = if version == 1 {
            reader.read_u64::<BigEndian>()?
        } else {
            reader.read_u32::<BigEndian>()? as u64
        };

        let _reserved = reader.read_u16::<BigEndian>()?; // reserved

        let reference_count = reader.read_u16::<BigEndian>()?;
        let mut references = Vec::new();

        for _ in 0..reference_count {
            let reference_type_and_size = reader.read_u32::<BigEndian>()?;
            let reference_type = (reference_type_and_size >> 31) as u8;
            let referenced_size = reference_type_and_size & 0x7FFFFFFF;

            let subsegment_duration = reader.read_u32::<BigEndian>()?;

            let starts_with_sap_and_sap_type_and_delta = reader.read_u32::<BigEndian>()?;
            let starts_with_sap = (starts_with_sap_and_sap_type_and_delta >> 31) as u8;
            let sap_type = ((starts_with_sap_and_sap_type_and_delta >> 28) & 0x7) as u8;
            let sap_delta_time = starts_with_sap_and_sap_type_and_delta & 0x0FFFFFFF;

            references.push(SidxReference {
                reference_type,
                referenced_size,
                subsegment_duration,
                starts_with_sap,
                sap_type,
                sap_delta_time,
            });
        }

        skip_bytes_to(reader, start + size)?;

        Ok(SidxBox {
            version,
            flags,
            reference_id,
            timescale,
            earliest_presentation_time,
            first_offset,
            references,
        })
    }
}

impl<W: Write> WriteBox<&mut W> for SidxBox {
    fn write_box(&self, writer: &mut W) -> Result<u64> {
        let size = self.box_size();
        BoxHeader::new(self.box_type(), size).write(writer)?;

        writer.write_u8(self.version)?;
        writer.write_u24::<BigEndian>(0)?; // flags

        writer.write_u32::<BigEndian>(self.reference_id)?;
        writer.write_u32::<BigEndian>(self.timescale)?;

        if self.version == 1 {
            writer.write_u64::<BigEndian>(self.earliest_presentation_time)?;
            writer.write_u64::<BigEndian>(self.first_offset)?;
        } else {
            writer.write_u32::<BigEndian>(self.earliest_presentation_time as u32)?;
            writer.write_u32::<BigEndian>(self.first_offset as u32)?;
        }

        writer.write_u16::<BigEndian>(0)?; // reserved

        let reference_count = self.references.len() as u16;
        writer.write_u16::<BigEndian>(reference_count)?;

        for reference in &self.references {
            let reference_type_and_size = ((reference.reference_type as u32) << 31)
                | (reference.referenced_size & 0x7FFFFFFF);
            writer.write_u32::<BigEndian>(reference_type_and_size)?;

            writer.write_u32::<BigEndian>(reference.subsegment_duration)?;

            let starts_with_sap_and_sap_type_and_delta = ((reference.starts_with_sap as u32) << 31)
                | ((reference.sap_type as u32) << 28)
                | (reference.sap_delta_time & 0x0FFFFFFF);
            writer.write_u32::<BigEndian>(starts_with_sap_and_sap_type_and_delta)?;
        }

        Ok(size)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn test_read_sidx_box() {
        let data: Vec<u8> = vec![
            0x00, 0x00, 0x00, 0x2c, // size = 44
            0x73, 0x69, 0x64, 0x78, // type = sidx
            0x00, // version = 0
            0x00, 0x00, 0x00, // flags = 0
            0x00, 0x00, 0x00, 0x01, // reference_id = 1
            0x00, 0x00, 0x03, 0xe8, // timescale = 1000
            0x00, 0x00, 0x00, 0x00, // earliest_presentation_time = 0
            0x00, 0x00, 0x00, 0x64, // first_offset = 100
            0x00, 0x00, // reserved = 0
            0x00, 0x01, // reference_count = 1
            0x80, 0x00, 0x00, 0x32, // reference_type, referenced_size = 50
            0x00, 0x00, 0x03, 0xe8, // subsegment_duration = 1000
            0x00, 0x00, 0x00, 0x00, // starts_with_sap, sap_type, sap_delta_time = 0
        ];
        let mut cursor = Cursor::new(data.clone());
        cursor.seek(SeekFrom::Start(8)).unwrap();
        let sidx_box = SidxBox::read_box(&mut cursor, data.len() as u64).unwrap();

        assert_eq!(sidx_box.version, 0);
        assert_eq!(sidx_box.reference_id, 1);
        assert_eq!(sidx_box.timescale, 1000);
        assert_eq!(sidx_box.earliest_presentation_time, 0);
        assert_eq!(sidx_box.first_offset, 100);
        assert_eq!(sidx_box.references.len(), 1);
        assert_eq!(sidx_box.references[0].referenced_size, 50);

        let mut writer = Cursor::new(vec![]);
        let size = sidx_box.write_box(&mut writer).unwrap();
        assert_eq!(size, data.len() as u64);

        assert_eq!(writer.into_inner(), data);
    }
}

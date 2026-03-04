package library

import (
	"encoding/binary"
	"io"
	"os"
	"strings"
)

// ExtractWavMetadata manually parses RIFF INFO chunks and other tags from a WAV file
func ExtractWavMetadata(path string) (title, artist, album string, err error) {
	f, err := os.Open(path)
	if err != nil {
		return "", "", "", err
	}
	defer f.Close()

	// Read RIFF header
	var header [12]byte
	if _, err := io.ReadFull(f, header[:]); err != nil {
		return "", "", "", err
	}

	if string(header[0:4]) != "RIFF" || string(header[8:12]) != "WAVE" {
		return "", "", "", nil
	}

	// Read chunks
	for {
		var chunkID [4]byte
		var chunkSize uint32
		if err := binary.Read(f, binary.LittleEndian, &chunkID); err != nil {
			if err == io.EOF {
				break
			}
			return title, artist, album, nil // Return what we found
		}
		if err := binary.Read(f, binary.LittleEndian, &chunkSize); err != nil {
			break
		}

		id := string(chunkID[:])
		
		// Handle ID3 chunk if it exists in WAV (some encoders do this)
		if id == "id3 " || id == "ID3 " {
			// We could try to parse this with id3v2 library, but for now let's skip
			f.Seek(int64(chunkSize), io.SeekCurrent)
			continue
		}

		if id == "LIST" {
			var listType [4]byte
			if _, err := io.ReadFull(f, listType[:]); err != nil {
				break
			}
			if string(listType[:]) == "INFO" {
				infoEnd := int64(chunkSize) - 4
				currentPos := int64(0)
				for currentPos < infoEnd {
					var subID [4]byte
					var subSize uint32
					if err := binary.Read(f, binary.LittleEndian, &subID); err != nil {
						break
					}
					if err := binary.Read(f, binary.LittleEndian, &subSize); err != nil {
						break
					}
					currentPos += 8

					if subSize > 1024*1024 { // Sanity check
						break
					}

					data := make([]byte, subSize)
					if _, err := io.ReadFull(f, data); err != nil {
						break
					}
					currentPos += int64(subSize)
					if subSize%2 != 0 {
						f.Seek(1, io.SeekCurrent)
						currentPos++
					}

					val := strings.TrimRight(string(data), "\x00")
					val = strings.TrimSpace(val)
					
					tag := string(subID[:])
					// Map RIFF INFO tags to standard fields
					// Ref: https://www.robotplanet.dk/programming/wav_format/
					switch tag {
					case "INAM", "titl":
						if title == "" { title = val }
					case "IART", "arch":
						if artist == "" { artist = val }
					case "IPRD", "prmp":
						if album == "" { album = val }
					}
				}
			} else {
				f.Seek(int64(chunkSize)-4, io.SeekCurrent)
			}
		} else {
			// Skip unknown chunks
			f.Seek(int64(chunkSize), io.SeekCurrent)
		}
		
		// Padding byte if chunk size is odd
		if chunkSize%2 != 0 {
			f.Seek(1, io.SeekCurrent)
		}
	}

	return title, artist, album, nil
}

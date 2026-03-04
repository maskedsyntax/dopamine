package library

import (
	"encoding/binary"
	"io"
	"os"
	"strings"
)

// ExtractWavMetadata manually parses RIFF INFO chunks from a WAV file
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
		return "", "", "", nil // Not a standard WAV
	}

	for {
		var chunkID [4]byte
		var chunkSize uint32
		if err := binary.Read(f, binary.LittleEndian, &chunkID); err != nil {
			if err == io.EOF {
				break
			}
			return "", "", "", err
		}
		if err := binary.Read(f, binary.LittleEndian, &chunkSize); err != nil {
			break
		}

		id := string(chunkID[:])
		if id == "LIST" {
			var listType [4]byte
			if _, err := io.ReadFull(f, listType[:]); err != nil {
				break
			}
			if string(listType[:]) == "INFO" {
				// We are in the INFO list
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
					switch string(subID[:]) {
					case "INAM":
						title = val
					case "IART":
						artist = val
					case "IPRD":
						album = val
					}
				}
			} else {
				f.Seek(int64(chunkSize)-4, io.SeekCurrent)
			}
		} else {
			f.Seek(int64(chunkSize), io.SeekCurrent)
		}
	}

	return title, artist, album, nil
}

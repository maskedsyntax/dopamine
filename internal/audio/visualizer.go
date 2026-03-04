package audio

import (
	"sync"

	"github.com/faiface/beep"
)

// VisualizerStreamer wraps a beep.Streamer and captures samples for visualization
type VisualizerStreamer struct {
	Streamer beep.Streamer
	buffer   []float64
	mu       sync.Mutex
	size     int
}

func NewVisualizerStreamer(s beep.Streamer, size int) *VisualizerStreamer {
	return &VisualizerStreamer{
		Streamer: s,
		buffer:   make([]float64, size),
		size:     size,
	}
}

func (vs *VisualizerStreamer) Stream(samples [][2]float64) (n int, ok bool) {
	n, ok = vs.Streamer.Stream(samples)
	if n > 0 {
		vs.mu.Lock()
		// Capture the last 'n' samples, or as many as fit in our buffer
		copyCount := n
		if copyCount > vs.size {
			copyCount = vs.size
		}
		
		// Shift old samples out and new ones in (simple rolling buffer)
		copy(vs.buffer, vs.buffer[copyCount:])
		for i := 0; i < copyCount; i++ {
			// Convert stereo to mono for simplicity
			vs.buffer[vs.size-copyCount+i] = (samples[i][0] + samples[i][1]) / 2.0
		}
		vs.mu.Unlock()
	}
	return n, ok
}

func (vs *VisualizerStreamer) Err() error {
	return vs.Streamer.Err()
}

func (vs *VisualizerStreamer) GetSamples() []float64 {
	vs.mu.Lock()
	defer vs.mu.Unlock()
	res := make([]float64, vs.size)
	copy(res, vs.buffer)
	return res
}

package audio

import (
	"os"
	"time"

	"github.com/faiface/beep"
	"github.com/faiface/beep/speaker"
	"github.com/faiface/beep/mp3"
	"github.com/faiface/beep/flac"
	"github.com/faiface/beep/wav"
)

type Engine struct {
	SampleRate beep.SampleRate
	Ctrl       *beep.Ctrl
	Streamer   beep.StreamSeekCloser
	Visualizer *VisualizerStreamer
	Format     beep.Format
}

func NewEngine() (*Engine, error) {
	sr := beep.SampleRate(44100)
	err := speaker.Init(sr, sr.N(time.Second/10))
	if err != nil {
		return nil, err
	}
	return &Engine{
		SampleRate: sr,
	}, nil
}

func (e *Engine) PlayFile(path string) error {
	f, err := os.Open(path)
	if err != nil {
		return err
	}

	var streamer beep.StreamSeekCloser
	var format beep.Format

	// Basic format detection by extension
	if endsWith(path, ".mp3") {
		streamer, format, err = mp3.Decode(f)
	} else if endsWith(path, ".flac") {
		streamer, format, err = flac.Decode(f)
	} else if endsWith(path, ".wav") {
		streamer, format, err = wav.Decode(f)
	}

	if err != nil {
		return err
	}

	e.Format = format
	
	// Resample to engine's sample rate to ensure correct playback speed
	resampled := beep.Resample(4, format.SampleRate, e.SampleRate, streamer)
	
	// Wrap with visualizer
	e.Visualizer = NewVisualizerStreamer(resampled, 1024)
	
	e.Streamer = streamer
	e.Ctrl = &beep.Ctrl{Streamer: e.Visualizer, Paused: false}

	speaker.Clear()
	speaker.Play(e.Ctrl)

	return nil
}

func (e *Engine) TogglePause() {
	if e.Ctrl != nil {
		speaker.Lock()
		e.Ctrl.Paused = !e.Ctrl.Paused
		speaker.Unlock()
	}
}

func (e *Engine) GetSamples() []float64 {
	if e.Visualizer != nil {
		return e.Visualizer.GetSamples()
	}
	return nil
}

func endsWith(s, suffix string) bool {
	if len(s) < len(suffix) {
		return false
	}
	return s[len(s)-len(suffix):] == suffix
}

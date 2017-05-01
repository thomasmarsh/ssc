// --------------------------------------------------------------------------
//
// Copyright (c) 2003 Thomas D. Marsh. All rights reserved.
//
// "SSC" is free software; you can redistribute it
// and/or use it and/or modify it under the terms of
// the "GNU General Public License" (GPL).
//
// --------------------------------------------------------------------------
//
// This code is taken (slightly modified) from Chromium BSU and is Copyright
// (c) 2000 Mark B. Allan. See the Chromium website at:
//
//      http://www.reptilelabour.com/software/chromium/

#include "audio.h"
#include "common.h"
#include "config.h"

// ---------------------------------------------------------------------------

Audio::Audio()
{
    fileNames[UNUSED] = "/usr/local/share/ssc/unused.wav";
    fileNames[BOOM] = "/usr/local/share/ssc/boom.wav";
    fileNames[EXPLOSION] = "/usr/local/share/ssc/exploBig.wav";
    fileNames[EXPLO_POP] = "/usr/local/share/ssc/exploPop.wav";
    fileNames[LIFE_ADD] = "/usr/local/share/ssc/life_add.wav";
    fileNames[LIFE_LOSE] = "/usr/local/share/ssc/life_lose.wav";
    fileNames[POWER] = "/usr/local/share/ssc/power.wav";
    fileNames[MUSIC_GAME] = "/usr/local/share/ssc/trance.wav";
}

Audio::~Audio()
{
}

void Audio::update()
{
}

void Audio::playSound(SoundType, float*, int)
{
}

void Audio::initSound()
{
}

void Audio::setSoundVolume(float)
{
}

// ---------------------------------------------------------------------------

SoundInfo::SoundInfo()
{
    type = Audio::EXPLOSION;
    pos[0] = 0.0;
    pos[1] = 0.0;
    pos[2] = 0.0;
    age = 1;
    back = next = 0;
}

SoundInfo::SoundInfo(Audio::SoundType t, float p[3], int a)
{
    type = t;
    pos[0] = p[0];
    pos[1] = p[1];
    pos[2] = p[2];
    age = a;
    back = next = 0;
}

// ---------------------------------------------------------------------------

AudioSDLMixer::AudioSDLMixer()
    : Audio()
{
    initSound();
}

AudioSDLMixer::~AudioSDLMixer()
{
    for (int i = 0; i < NUM_SOUND_TYPES; i++) {
        Mix_FreeChunk(sounds[i]);
    }
    Mix_CloseAudio();
}

void AudioSDLMixer::initSound()
{
    Config* conf = Config::getInstance();

    if (!conf->playSound() && !conf->playMusic()) {
        return;
    }

    if (Mix_OpenAudio(22050, AUDIO_S16, 2, 512) < 0) {
        fprintf(stderr,
                "Could not initialize SDL mixer: %s\n",
                SDL_GetError());
        exit(1);
    }

    for (int i = 0; i < NUM_SOUND_TYPES; i++) {
        sounds[i] = Mix_LoadWAV(fileNames[i]);
    }

    Mix_ReserveChannels(1);
    Mix_PlayChannel(0, sounds[MUSIC_GAME], -1);

    if (conf->playMusic()) {
        Mix_Volume(0, (int)(MIX_MAX_VOLUME * conf->musicVol()));
    } else {
        Mix_Volume(0, 0);
    }

    // atexit(Mix_CloseAudio);
}

void AudioSDLMixer::playSound(SoundType type, float pos[3], int)
{
    if (Config::getInstance()->playSound()) {
        Mix_PlayChannel(-1, sounds[type], 0);
    }
}

void AudioSDLMixer::setSoundVolume(float value)
{
    if (Config::getInstance()->playSound()) {
        for (int i = 1; i < MIX_CHANNELS; i++) {
            Mix_Volume(i, (int)(MIX_MAX_VOLUME * value));
        }
    }
}

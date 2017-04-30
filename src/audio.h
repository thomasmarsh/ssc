// ---------------------------------------------------------------------------
//
// Copyright (c) 2003 Thomas D. Marsh. All rights reserved.
//
// "SSC" is free software; you can redistribute it
// and/or use it and/or modify it under the terms of
// the "GNU General Public License" (GPL).
//
// ---------------------------------------------------------------------------
//
// This code is taken (slightly modified) from Chromium BSU and is Copyright
// (c) 2000 Mark B. Allan. See the Chromium website at:
//
//      http://www.reptilelabour.com/software/chromium/


#ifndef SSC_AUDIO_H
#define SSC_AUDIO_H

class Audio
{
public:
        enum SoundType
        {
                UNUSED,
                BOOM,
                EXPLOSION,
                EXPLO_POP,
                LIFE_ADD,
                LIFE_LOSE,
                POWER,
                MUSIC_GAME,
                NUM_SOUND_TYPES
        };

        Audio();
        virtual ~Audio();

        virtual void update();
        virtual void playSound(SoundType type, float *pos, int age=0);
        virtual void setSoundVolume(float);

protected:
        virtual void initSound();

        char *fileNames[NUM_SOUND_TYPES];
};

class SoundInfo
{
public:
        SoundInfo();
        SoundInfo(Audio::SoundType t, float p[3], int a);

        Audio::SoundType type;
        float pos[3];
        int age;
        SoundInfo *next;
        SoundInfo *back;
};

#include <SDL/SDL_mixer.h>

class AudioSDLMixer : public Audio
{
public:
        AudioSDLMixer();
        ~AudioSDLMixer();

        void playSound(SoundType type, float *pos, int age=0);
        void setSoundVolume(float);

protected:
        void initSound();

private:
        Mix_Chunk *sounds[NUM_SOUND_TYPES];
};

#endif // SSC_AUDIO_H

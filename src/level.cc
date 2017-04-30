// --------------------------------------------------------------------------
//
// Copyright (c) 2003 Thomas D. Marsh. All rights reserved.
//
// "SSC" is free software; you can redistribute it
// and/or use it and/or modify it under the terms of
// the "GNU General Public License" (GPL).
//
// --------------------------------------------------------------------------

#include "level.h"
#include "asteroid.h"
#include "blackhole.h"
#include "bogey.h"
#include "fatso.h"
#include "global.h"
#include "lunatic.h"
#include "smarty.h"

static float pos[3] = { 0, 0, 0 };

Level::Level()
{
}

Level::~Level()
{
}

static unsigned int level = 1;
void Level::setLevel(int n)
{
    level = n - 1;
    (*this)++;
}

bool Level::completed()
{
    return (Bogey::num_alive == 0);
}

void Level::operator++(int)
{
    unsigned int i;

    Model::getInstance()->clearLevel(false);

    level++;

    Global::audio->playSound(Audio::POWER, pos);
    for (i = 0; i < ((MAX_BOGEYS * level) >> 4); i++) {
        (void)new Bogey((rand() % Screen::maxX() / 50) * (double)50,
            (rand() % Screen::maxY() / 50) * (double)50);
    }
    for (i = 0; i < 15; i++) {
        (void)new Asteroid(drand48() * 100 + 50);
    }

    for (i = 0; i < (level - 1); i++) {
        (void)new Smarty();
    }

    //      for (i = 0; i < 15; i++) (void) new Fatso();
    for (i = 0; i < (5 * level); i++) {
        (void)new Lunatic();
    }
}

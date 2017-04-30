// --------------------------------------------------------------------------
//
// Copyright (c) 2003 Thomas D. Marsh. All rights reserved.
//
// "SSC" is free software; you can redistribute it
// and/or use it and/or modify it under the terms of
// the "GNU General Public License" (GPL).
//
// --------------------------------------------------------------------------

#include "asteroid.h"
#include "draw.h"
#include "game.h"
#include "graph.h"

int main(int argc, char** argv)
{
    Config* conf = Config::getInstance();
    conf->handleArguments(argc, argv);

    srand(time(NULL));
    Global::audio = new AudioSDLMixer();
    Global::audio->setSoundVolume(conf->soundVol());

    dInitODE();
    Screen::init();

    GraphicContext().createWindow(
        "Testing",
        Screen::mDisplay.x, Screen::mDisplay.y,
        conf->getColorDepth(),
        conf->fullscreen());

    Game::getInstance()->loop();

    // not reached
    return 0;
}

// --------------------------------------------------------------------------
//
// Copyright (c) 2003 Thomas D. Marsh. All rights reserved.
//
// "SSC" is free software; you can redistribute it
// and/or use it and/or modify it under the terms of
// the "GNU General Public License" (GPL).
//
// --------------------------------------------------------------------------

#include "game.h"
#include "screen.h"

const int FPS_SZ = 100;

inline double fps(double wait)
{
    static double times[FPS_SZ], avg;
    static int c = 0, i;

    avg = 0;
    c = (c + 1) % FPS_SZ;
    times[c] = wait;

    for (i = 0; i < FPS_SZ; ++i) {
        avg += times[i];
    }

    return 1000 / (avg / FPS_SZ);
};

double mFramerate;

Game::Game()
    : mModel(Model::getInstance())
    , dt(0)
    , mTime1(SDL_GetTicks())
    , mTime2(SDL_GetTicks())
    ,
#ifdef WINDOWS
    mWait(10)
#else
    mWait(1)
#endif
{
    Global::ship = new Ship(100, 100);
    mModel.setPlayerPos();
    mLevel.setLevel(1);
    mMenuHandler.setMenu(&mGameMenu);
    mMenuHandler.setController(&mController);
    mPlayHandler.setController(&mController);
    setMode(MENU);
}

void Game::setMode(GameMode mode)
{
    if (mMode == mode) {
        return;
    }

    mMode = mode;

    switch (mMode) {
    case PLAY:
        mModel.startGame();
        mLevel.setLevel(1);
        mController.setHandler(&mPlayHandler);
        mController.unPause();
        Global::audio->setSoundVolume(Config::getInstance().soundVol());
        break;
    case MENU:
        mController.setHandler(&mMenuHandler);
        Global::audio->setSoundVolume(0);
        break;
    }
}

void Game::loop()
{
    while (1) {
        if (!mController.isPaused() && (dt > 0)) {
            mModel.update(dt);
            if (mLevel.completed())
                mLevel++;
        } else {
            mModel.draw(dt, false);
        }

        mController.poll(dt);
        delay();

        if (mController.wantExit()) {
            if (mMode == MENU) {
                break;
            } else {
                setMode(MENU);
            }
        }
    }

    SDL_Quit();
}

void Game::delay()
{
    if (mController.isPaused()) {
        SDL_Delay(100);
    } else {
        SDL_Delay(mWait);
    }

    mTime1 = SDL_GetTicks();
    double delta = mTime1 - mTime2;
    dt = delta * (mController.slowMotion() ? .025 : .05);
    mFramerate = fps(delta);
    mTime2 = SDL_GetTicks();

    if (!mController.isPaused() && (dt > 1.5)) {
        static bool speedWarn = true;
        if (speedWarn && (mFramerate < 50)) {
            // TODO: add some flashing in-game text with the
            // framerate warning
            fprintf(stderr,
                    "Could not maintain framerate 50 fps! (dt=%.2f)\n",
                    dt);
            speedWarn = false;
        }
        dt = 1.5;
    }
    static double cnt = 0;
    if (cnt > 50) {
        fprintf(stderr, "fps: %.2f\n", mFramerate), cnt = 0;
    }
    cnt += dt;

#ifndef WINDOWS
    if (!mController.slowMotion()) {
        if ((mWait > 1) && dt > 1.1) {
            mWait--;
        } else if ((mWait < 20) && (dt < .3)) {
            mWait++;
        }
    }
#endif // WINDOWS
}

// --------------------------------------------------------------------------
//
// Copyright (c) 2003 Thomas D. Marsh. All rights reserved.
//
// "SSC" is free software; you can redistribute it
// and/or use it and/or modify it under the terms of
// the "GNU General Public License" (GPL).
//
// --------------------------------------------------------------------------

#include "control.h"
#include "global.h"
#include "hud.h"
#include "model.h"

static SDL_Event event;

bool mGodMode = false,
     mKill = false,
     mSetZ = false,
     mDrawP = false,
     mDrawBLINE = false;

Controller::Controller()
{
    mQuit = mPause = mSlowMo = false;
}

Controller::~Controller()
{
}

void Controller::keyUp(int keysym) {
    switch (keysym) {
    case SDLK_PAUSE:
        mPause = !mPause;
        break;
    case SDLK_r:
        HUD::getInstance()->toggleRadar();
        break;
    case SDLK_c:
        Model::getInstance()->cycleCameraView();
        break;
    case SDLK_s:
        mSlowMo = !mSlowMo;
        break;
    case SDLK_g:
        mGodMode = !mGodMode;
        break;
    case SDLK_m:
        Global::ship->upgrade();
        break;
    case SDLK_k:
        mKill = !mKill;
        break;
    case SDLK_z:
        mSetZ = !mSetZ;
        break;
    case SDLK_p:
        mDrawP = !mDrawP;
        break;
    case SDLK_l:
        mDrawBLINE = !mDrawBLINE;
        break;
    case SDLK_y:
        Model::getInstance()->clearLevel();
        break;
    default:
        mHandler->releaseKey((SDLKey)keysym);
        break;
    }
}

void Controller::poll(double dt)
{
    while (SDL_PollEvent(&event)) {
        switch (event.type) {
        case SDL_QUIT:
            mQuit = true;
            return;
        case SDL_KEYDOWN:
            if (event.key.keysym.sym == SDLK_ESCAPE) {
                mQuit = true;
                return;
            } else if (event.key.keysym.sym == SDLK_F1) {
                SDL_WM_ToggleFullScreen(SDL_GetVideoSurface());
            } else {
                mHandler->pressKey(event.key.keysym.sym);
            }
            break;
        case SDL_KEYUP:
            keyUp(event.key.keysym.sym);
            break;

        case SDL_MOUSEMOTION:
            mHandler->mouseMotion(event.motion.x, event.motion.y);
            break;

        case SDL_MOUSEBUTTONDOWN:
            mHandler->pressMouseButton(event.button.button);
            mHandler->mouseMotion(event.button.x, event.button.y);
            break;
        case SDL_MOUSEBUTTONUP:
            mHandler->releaseMouseButton(event.button.button);
            mHandler->mouseMotion(event.button.x, event.button.y);
            break;
        }

        if (mQuit) {
            return;
        }
    }

    if (dt > 0) {
        mHandler->process(dt);
    }
}

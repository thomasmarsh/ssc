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
#include "model.h"
#include "hud.h"
#include "global.h"

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

void Controller::poll(double dt)
{
        while (SDL_PollEvent(&event))
        {
                switch (event.type)
                {
                case SDL_QUIT:
                        mQuit = true;
                        return;
                case SDL_KEYDOWN:
                        if (event.key.keysym.sym == SDLK_ESCAPE)
                        {
                                mQuit = true;
                                return;
                        }
                        else if (event.key.keysym.sym == SDLK_F1)
                                SDL_WM_ToggleFullScreen(SDL_GetVideoSurface());
                        else
                                mHandler->pressKey(event.key.keysym.sym);
                        break;
                case SDL_KEYUP:
                        if (event.key.keysym.sym == SDLK_PAUSE)
                                mPause = !mPause;
                        else if (event.key.keysym.sym == SDLK_r)
                                HUD::getInstance()->toggleRadar();
                        else if (event.key.keysym.sym == SDLK_c)
                                Model::getInstance()->cycleCameraView();
                        else if (event.key.keysym.sym == SDLK_s)
                                mSlowMo = !mSlowMo;
                        else if (event.key.keysym.sym == SDLK_g)
                                mGodMode = !mGodMode;
                        else if (event.key.keysym.sym == SDLK_m)
                                Global::ship->upgrade();
                        else if (event.key.keysym.sym == SDLK_k)
                                mKill = !mKill;
                        else if (event.key.keysym.sym == SDLK_z)
                                mSetZ = !mSetZ;
                        else if (event.key.keysym.sym == SDLK_p)
                                mDrawP = !mDrawP;
                        else if (event.key.keysym.sym == SDLK_l)
                                mDrawBLINE = !mDrawBLINE;
                        else if (event.key.keysym.sym == SDLK_y)
                                Model::getInstance()->clearLevel();
                        else
                                mHandler->releaseKey(event.key.keysym.sym);
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

                if (mQuit) return;
        }

        if (dt > 0) mHandler->process(dt);
}

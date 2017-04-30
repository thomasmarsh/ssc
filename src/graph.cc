// --------------------------------------------------------------------------
//
// Copyright (c) 2003 Thomas D. Marsh. All rights reserved.
//
// "SSC" is free software; you can redistribute it
// and/or use it and/or modify it under the terms of
// the "GNU General Public License" (GPL).
//
// --------------------------------------------------------------------------

#include "graph.h"
#include "draw.h"
#include "hud.h"

GraphicContext::GraphicContext()
{
}

GraphicContext::~GraphicContext()
{
}

extern void initTexture();

void GraphicContext::initGL()
{
    // depth test
    glEnable(GL_DEPTH_TEST);
    glClearDepth(1000);
    glDepthFunc(GL_LESS);

    // perspective calculations
    glHint(GL_PERSPECTIVE_CORRECTION_HINT, GL_NICEST);

    glEnable(GL_BLEND);
    glBlendFunc(GL_SRC_ALPHA, GL_ONE_MINUS_SRC_ALPHA);
    glShadeModel(GL_SMOOTH);
    glPixelStorei(GL_UNPACK_ALIGNMENT, 1);

    glClearColor(0.0, 0.0, 0.0, 1.0);

    glFlush();
}

bool GraphicContext::createWindow(const char* title, int w, int h, int b, bool fullscreen)
{
    if (SDL_Init(SDL_INIT_VIDEO) < 0) {
        fprintf(stderr, "Couldn't initialize SDL: %s\n", SDL_GetError());
        return false;
    }

    HUD::init();

    int flags = SDL_OPENGL;
    if (fullscreen)
        flags |= SDL_FULLSCREEN;
    flags |= SDL_GL_DOUBLEBUFFER;
    flags |= SDL_HWPALETTE;

    const SDL_VideoInfo* videoInfo = SDL_GetVideoInfo();

    if (videoInfo == NULL) {
        fprintf(stderr, "Video query failed: %s\n", SDL_GetError());
        SDL_Quit();
        exit(-1);
    }

    if (videoInfo->hw_available)
        flags |= SDL_HWSURFACE;
    if (videoInfo->blit_hw)
        flags |= SDL_HWACCEL;

    int rs(0), gs(0), bs(0), ds(0);
    if (b == 24) {
        rs = gs = bs = 8;
        ds = 16;
    } else if (b == 16) {
        rs = bs = 5;
        gs = 6;
        ds = 16;
    } else {
        fprintf(stderr, "Unknow color depth: %d\n", b);
        SDL_Quit();
        exit(-1);
    }

    SDL_GL_SetAttribute(SDL_GL_RED_SIZE, rs);
    SDL_GL_SetAttribute(SDL_GL_GREEN_SIZE, gs);
    SDL_GL_SetAttribute(SDL_GL_BLUE_SIZE, bs);
    SDL_GL_SetAttribute(SDL_GL_DEPTH_SIZE, ds);
    SDL_GL_SetAttribute(SDL_GL_DOUBLEBUFFER, 1);

    SDL_Surface* main_surface = SDL_SetVideoMode(w, h, 0, flags);
    if (main_surface == NULL) {
        fprintf(stderr, "Couldn't set video mode: %s\n", SDL_GetError());
        SDL_Quit();
        exit(-1);
    }

    initGL();

    resizeScene(w, h);

    return true;
}

void GraphicContext::resizeScene(int w, int h)
{

    if (h == 0)
        h = 1;

    glViewport(0, 0, w, h);
    draw::setupGL();
}

void GraphicContext::closeWindow()
{
    SDL_Quit();
}

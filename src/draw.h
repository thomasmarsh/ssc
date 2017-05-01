// --------------------------------------------------------------------------
//
// Copyright (c) 2003 Thomas D. Marsh. All rights reserved.
//
// "SSC" is free software; you can redistribute it
// and/or use it and/or modify it under the terms of
// the "GNU General Public License" (GPL).
//
// --------------------------------------------------------------------------

#ifndef SSC_DRAW_H
#define SSC_DRAW_H

#include <SDL/SDL.h>
#ifdef __APPLE__
#include <GLUT/glut.h>
#include <OpenGL/gl.h>
#include <OpenGL/glu.h>
#else
#ifdef _WIN32
#include <windows.h>
#endif
#include <GL/gl.h>
#include <GL/glu.h>
#include <GL/glut.h>
#endif

#include "common.h"
#include "screen.h"

namespace draw {
enum DrawMode { DRAW_2D,
                DRAW_3D };

extern DrawMode mDrawMode;
extern GLUquadricObj* mSphere;

inline void setupGL()
{
    glEnable(GL_DEPTH_TEST);
    glEnable(GL_LIGHTING);
    glEnable(GL_LIGHT0);
    glEnable(GL_LIGHT1);
    glEnable(GL_CULL_FACE);
    glEnable(GL_COLOR_MATERIAL);
    glEnable(GL_NORMALIZE);
    float specular[] = { .1, .1, .1, 1 };
    float shininess[] = { 1 };

    glMaterialfv(GL_FRONT_AND_BACK, GL_SPECULAR, specular);
    glMaterialfv(GL_FRONT_AND_BACK, GL_SHININESS, shininess);
    glPolygonMode(GL_FRONT_AND_BACK, GL_FILL);
    glColorMaterial(GL_FRONT_AND_BACK, GL_AMBIENT_AND_DIFFUSE);

    glMatrixMode(GL_PROJECTION);
    glLoadIdentity();
    gluPerspective(
        Screen::fieldOfView(),
        Screen::aspectRatio(),
        Screen::zNear(),
        Screen::zFar());

    glMatrixMode(GL_MODELVIEW);
    glLoadIdentity();
}

inline void setMode(DrawMode newMode)
{
    assert(newMode != mDrawMode);
    mDrawMode = newMode;

    switch (mDrawMode) {
    case DRAW_2D:
        glDisable(GL_DEPTH_TEST);
        glDisable(GL_DEPTH_TEST);
        glDisable(GL_LIGHTING);
        glDisable(GL_LIGHT0);
        glDisable(GL_LIGHT1);

        glMatrixMode(GL_PROJECTION);
        glPushMatrix();
        glLoadIdentity();
        gluOrtho2D(0, Screen::mDisplay.x, 0, Screen::mDisplay.y);
        glMatrixMode(GL_MODELVIEW);
        glPushMatrix();
        glLoadIdentity();
        break;

    case DRAW_3D:
        glMatrixMode(GL_MODELVIEW);
        glPopMatrix();
        glMatrixMode(GL_PROJECTION);
        glPopMatrix();
        glMatrixMode(GL_MODELVIEW);

        glEnable(GL_DEPTH_TEST);
        glEnable(GL_LIGHTING);
        glEnable(GL_LIGHT0);
        glEnable(GL_LIGHT1);
        break;
    }
}

inline void flush() { glFlush(); }
inline void flipBuffers() { SDL_GL_SwapBuffers(); }
inline void clearScreen() { glClear(GL_COLOR_BUFFER_BIT | GL_DEPTH_BUFFER_BIT); }
inline void setColor(float r, float g, float b) { glColor4f(r, g, b, 1.0); }
inline void setColor(float r, float g, float b, float a) { glColor4f(r, g, b, a); }

inline void checkErrors()
{
    GLenum gl_error;
    char* sdl_error;

    gl_error = glGetError();
    if (gl_error != GL_NO_ERROR) {
        fprintf(stderr, "ERROR: OpenGl: %s\n", gluErrorString(gl_error));
    }
    sdl_error = SDL_GetError();
    if (sdl_error[0] != '\0') {
        fprintf(stderr, "ERROR: SDL: %s\n", sdl_error);
        SDL_ClearError();
    }
}

inline void sphere(Coord3<double>& pos, double r, int add = 3, int b = 10)
{
    glPushMatrix();
    glTranslated(pos.x, -pos.y, pos.z);
    gluQuadricDrawStyle(mSphere, GLU_FILL);
    gluQuadricNormals(mSphere, GLU_SMOOTH);
    gluSphere(mSphere, r, (int)r + add, b);
    glPopMatrix();
}

inline void line(double x1, double y1, double z1,
                 double x2, double y2, double z2)
{
    glPushMatrix();
    glBegin(GL_LINES);
    glNormal3d(0, 0, 1);
    glVertex3d(x1, -y1, z1);
    glNormal3d(0, 0, 1);
    glVertex3d(x2, -y2, z2);
    glEnd();
    glPopMatrix();
}

inline void line(int x1, int y1, int x2, int y2)
{
    glPushMatrix();
    glBegin(GL_LINES);
    switch (mDrawMode) {
    case DRAW_2D:
        glVertex2i(x1, Screen::mDisplay.y - y1);
        glVertex2i(x2, Screen::mDisplay.y - y2);
        break;

    case DRAW_3D:
        glNormal3i(0, 0, 1);
        glVertex3i(x1, -y1, 0);
        glNormal3i(0, 0, 1);
        glVertex3i(x2, -y2, 0);
        break;
    }
    glEnd();
    glPopMatrix();
}

inline void rect(int x1, int y1, int x2, int y2, int extra = 0, bool transY = false)
{
    if (transY)
        y1 = Screen::mDisplay.y - y1, y2 = Screen::mDisplay.y - y2;
    glBegin(GL_LINES);
    glVertex2i(x1, y1 + extra);
    glVertex2i(x1, y2 - extra);
    glVertex2i(x1 - extra, y2);
    glVertex2i(x2 + extra, y2);
    glVertex2i(x2, y2 - extra);
    glVertex2i(x2, y1 + extra);
    glVertex2i(x1 - extra, y1);
    glVertex2i(x2 + extra, y1);
    glEnd();
}

inline void box(int x1, int y1, int x2, int y2)
{
    glBegin(GL_QUADS);
    glVertex2i(x1, Screen::mDisplay.y - y1);
    glVertex2i(x1, Screen::mDisplay.y - y2);
    glVertex2i(x2, Screen::mDisplay.y - y2);
    glVertex2i(x2, Screen::mDisplay.y - y1);
    glEnd();
}

inline void startPoints() { glBegin(GL_POINTS); }

inline void point(int x, int y, int z = 0)
{
    switch (mDrawMode) {
    case DRAW_2D:
        glVertex2i(x, Screen::mDisplay.y - y);
        break;
    case DRAW_3D:
        glNormal3i(0, 0, 1);
        glVertex3i(x, -y, z);
    }
}

inline void endPoints() { glEnd(); }

inline void startTriangles() { glBegin(GL_TRIANGLES); }
inline void normal(Coord3<double>& pos) { glNormal3d(pos.x, -pos.y, pos.z); }
inline void trianglePoint(double x, double y, double z) { glVertex3d(x, -y, z); }
inline void endTriangles() { glEnd(); }
};

#endif // SSC_DRAW_H

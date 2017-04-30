// --------------------------------------------------------------------------
//
// Copyright (c) 2003 Thomas D. Marsh. All rights reserved.
//
// "SSC" is free software; you can redistribute it
// and/or use it and/or modify it under the terms of
// the "GNU General Public License" (GPL).
//
// --------------------------------------------------------------------------

#include "fatso.h"
#include "draw.h"
#include "graph.h"

int FATSO = -1;

Fatso::Fatso()
        : ScreenObject(FATSO_TYPE, FATSO_RADIUS, FATSO_MASS, 100,
                       (double) (rand() % (Screen::maxX()-2*FATSO_RADIUS)) + FATSO_RADIUS,
                       (double) (rand() % (Screen::maxY()-2*FATSO_RADIUS)) + FATSO_RADIUS,
                       0,
                       0, 0, 0)
{
        r = .8, g = .7, b = .6;
        setState(ALIVE);

        if (FATSO == -1)
        {
                FATSO = glGenLists(1);
                glNewList(FATSO, GL_COMPILE);
                GLUquadricObj *m = gluNewQuadric();
                gluSphere(m, radius, radius+3, 10);
                gluDeleteQuadric(m);
                glEndList();
        }
}

Fatso::~Fatso()
{
}


void Fatso::draw()
{
        glPushMatrix();
        glTranslated(mPosition.x, -mPosition.y, mPosition.z);
        draw::setColor(r,g,b,1.0);
        glCallList(FATSO);
        glPopMatrix();
}

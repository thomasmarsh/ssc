// --------------------------------------------------------------------------
//
// Copyright (c) 2003 Thomas D. Marsh. All rights reserved.
//
// "SSC" is free software; you can redistribute it
// and/or use it and/or modify it under the terms of
// the "GNU General Public License" (GPL).
//
// --------------------------------------------------------------------------

#include "missile.h"
#include "screen.h"
#include "draw.h"
#include "common.h"

const double            MAX_MISSILE_AGE = 80;
const double            MISSILE_MASS    = 25.0;
const unsigned int      MISSILE_SPEED   = 7;
const unsigned int      MISSILE_RADIUS  = 3;


void Missile::move(double dt)
{
        ScreenObject::move(dt);

        age += dt;

        if (age > MAX_MISSILE_AGE)
                setState(DEAD);
}

Missile::Missile(ObjectType owner,
                 double rotation,
                 double x, double y, double z,
                 double fx, double fy, double fz)
        : ScreenObject(MISSILE_TYPE, MISSILE_RADIUS, MISSILE_MASS, 100,
                       x, y, z,
                       fx+sin(rotation)*MISSILE_SPEED,
                       fy-cos(rotation)*MISSILE_SPEED, fz),
                     age(0), owner_type(owner)
{
        switch (owner_type)
        {
        case BOGEY_TYPE:
                r = b = 0;
                g = 1;
                break;
        case PLAYER_TYPE:
                r = 1;
                g = b = 0;
        default:
                break;
        }

        setState(ALIVE);
}

Missile::~Missile()
{
}

bool Missile::collision(ScreenObject &obj) { return true; }


const int NUM_AMMO_TYPES = 5;

GLuint ammoTex[NUM_AMMO_TYPES];

#include "glpng.h"

inline
void DrawPlayerMissile(Coord3<double> &mPosition, bool player=true)
{
        // load textures
        static bool need_tex = true;
        double radius = MISSILE_RADIUS * 1.5;
        if (need_tex)
        {
                char filename[256];
                pngInfo tmpInfo;
                for (int i = 0; i < NUM_AMMO_TYPES; ++i)
                {
                        snprintf(filename, 256, "../data/enemyAmmo%02d.png", i);
                        ammoTex[i] = pngBind(filename, PNG_NOMIPMAPS, PNG_ALPHA,
                                &tmpInfo, GL_CLAMP, GL_LINEAR, GL_LINEAR);
                        if (!ammoTex[i])
                        {
                                fprintf(stderr, "can't load %s\n", filename);
                                abort();
                        }
                }
                need_tex = false;
        }
        glDisable(GL_DEPTH_TEST);
        glEnable(GL_TEXTURE_2D);
        glBlendFunc(GL_SRC_ALPHA, GL_ONE);
        for (int i = 0; i < NUM_AMMO_TYPES; ++i)
        {
                if (player) glColor4d(1, .5, .2, .7);
                else glColor4d(0, 1, 0, .8);
                glBindTexture(GL_TEXTURE_2D, ammoTex[i]);
                glPushMatrix();
                glTranslated(mPosition.x, -mPosition.y, mPosition.z);
                glRotated(rand() % 360, 0, 0, 1);
//              glRotated((rand() % 4) * 90, 0, 0, 1);
                glBegin(GL_QUADS);
                glTexCoord2i(0, 0); glNormal3i(0, 0, 1);
                glVertex3d(-radius, radius, 0);
                glTexCoord2i(0, 1); glNormal3i(0, 0, 1);
                glVertex3d(-radius, -radius, 0);
                glTexCoord2i(1, 1); glNormal3i(0, 0, 1);
                glVertex3d(radius, -radius, 0);
                glTexCoord2i(1, 0); glNormal3i(0, 0, 1);
                glVertex3d(radius,  radius, 0);
                glEnd();
                glPopMatrix();
        }
        glBlendFunc(GL_SRC_ALPHA, GL_ONE_MINUS_SRC_ALPHA);
        glEnable(GL_DEPTH_TEST);
        glDisable(GL_TEXTURE_2D);
}

void Missile::draw()
{
        if (owner_type == PLAYER_TYPE)
                DrawPlayerMissile(mPosition);
        else
                DrawPlayerMissile(mPosition, false);
}

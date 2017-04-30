// --------------------------------------------------------------------------
//
// Copyright (c) 2003 Thomas D. Marsh. All rights reserved.
//
// "SSC" is free software; you can redistribute it
// and/or use it and/or modify it under the terms of
// the "GNU General Public License" (GPL).
//
// --------------------------------------------------------------------------

#include "smarty.h"
#include "draw.h"
#include "global.h"
#include "model.h"

NeuralNetwork<4, 6, 2> Smarty::brain;

inline double fixR(double r)
{
    while (r > D_PI)
        r -= D_PI;
    while (r < 0)
        r += D_PI;
    return r;
}

inline double FUZZ(double r) { return fixR(r) / D_PI; }
inline double DFUZ(double n) { return n * D_PI; }

extern bool mDrawP;

void Smarty::accelerate(double amt)
{
    if (mDrawP)
        for (int i = 0; i < (rand() % 30); i++) {
            float p = RAD((rand() % 61) - 30);
            double r, g, b;
            r = 1;
            g = (double)(rand() % 80) / 100;
            b = (double)(rand() % 30) / 100;
            double t = (double)((rand() % 120)) / 5.0;
            addParticle(mPosition.x - radius * sin(rotation) + ((rand() % 7) - 3),
                mPosition.y + radius * cos(rotation) + ((rand() % 7) - 3),
                mPosition.z + (rand() % 7) - 3,
                -sin(rotation + p) * speed * .5,
                cos(rotation + p) * speed * .5,
                sin(p) * 15 * .5,
                r, g, b, t);
        }
    ScreenObject::accelerate(amt);
}

int SMARTY = -1;

Smarty::Smarty()
    : ScreenObject(SMARTY_TYPE, SMARTY_RADIUS, SMARTY_MASS, SMARTY_MAX_SPEED,
          (double)(rand() % Screen::maxX()),
          (double)(rand() % Screen::maxY()),
          0,
          0, 0, 0)
{
    mLastPosition.set(mPosition);
    mLastForce.set(mVelocity);
    mLastShipPos.set(Global::ship->mPosition);

    r = .2, g = .3, b = .4;
    setState(ALIVE);
    if (SMARTY == -1) {
        SMARTY = glGenLists(1);
        glNewList(SMARTY, GL_COMPILE);
        GLUquadricObj* m = gluNewQuadric();
        gluSphere(m, radius, radius + 3, 10);
        gluDeleteQuadric(m);
        glEndList();
    }
}

Smarty::~Smarty()
{
}

void Smarty::rotate(double amt)
{
    rotation += amt;

    while (rotation > D_PI)
        rotation -= D_PI;
    while (rotation < 0)
        rotation += D_PI;
}

#define LOG(a)                  \
    fprintf(stderr, "%s\n", a); \
    fflush(stderr)

inline double fzAngle(Coord3<double>& c)
{
    return FUZZ(atan2(c.x, -c.y));
}
inline double fzMag(double m, Coord3<double>& c) { return c.length() / m; }

void Smarty::move(double dt)
{
    static Coord3<double> mDiff, ppp;
    if (isAlive()) {
        mDiff = Global::ship->mPosition - mPosition;
        // train based on last position
        ppp = mDiff - mLastPosition;
        double d = ppp.length();
        if (d >= 1)
            d = 1 / (d * d * d);
        else
            d = 0;
        d = 1 - d;
        double data1[4] = { fzAngle(mDiff), fzAngle(mVelocity), fzMag(mMaxSpeed, mVelocity), dt };
        double result1[2] = { fzAngle(mDiff), d };
        brain.train(data1, result1, .000001, 0.003, 100);

        // get the ship differential vector
        mLastPosition.set(mDiff);
        mLastForce.set(mVelocity);
        mLastShipPos.set(Global::ship->mPosition);

        // build input and output vectors
        double data2[4] = { fzAngle(mDiff), fzAngle(mVelocity), fzMag(mMaxSpeed, mVelocity), dt };
        double result2[2] = { 0, 0 };

        brain.run(data2, result2);

        // behave based upon output

        rotation = DFUZ(result2[0]);
        accelerate(result2[1] * dt * .5);
    } else if (isDying()) {
        if (mExplosion.finished)
            setState(DEAD);
        mExplosion.move(dt);
    }
    ScreenObject::move(dt);
}

bool Smarty::collision(ScreenObject& obj)
{
    return true;
    static float pos[3] = { 0.0, 0.0, 0.0 };

    if (isAlive()) {
        if ((obj.type() == MISSILE_TYPE) && (obj.ownerType() == PLAYER_TYPE)) {
            setState(DYING);
            Global::audio->playSound(Audio::EXPLO_POP, pos);
            mExplosion.init(*this);
        }
        return true;
    }
    return false;
}

void Smarty::draw()
{
    if (isAlive()) {
        glPushMatrix();
        glTranslated(mPosition.x, -mPosition.y, mPosition.z);
        draw::setColor(r, g, b);
        glCallList(SMARTY);
        glPopMatrix();
    } else if (isDying())
        mExplosion.draw();
}

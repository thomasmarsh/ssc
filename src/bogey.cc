// --------------------------------------------------------------------------
//
// Copyright (c) 2003 Thomas D. Marsh. All rights reserved.
//
// "SSC" is free software; you can redistribute it
// and/or use it and/or modify it under the terms of
// the "GNU General Public License" (GPL).
//
// --------------------------------------------------------------------------

#include "bogey.h"
#include "draw.h"
#include "global.h"
#include "model.h"
#include "screen.h"

using namespace std;

unsigned int Bogey::num_alive;
static float pos[3] = { 0.0, 0.0, 0.0 };

inline void calcRotationPoints(double* x, double* y, double rot, int radius)
{
    *x = rint(sin(rot) * radius);
    *y = rint(-cos(rot) * radius);
}

int BOGEY = -1;

Bogey::Bogey(double x, double y, double z)
    : FlockMember(BOGEY_TYPE,
                  BOGEY_SHIELD_RADIUS,
                  BOGEY_MASS,
                  7,
                  x, y, z, 0, 0, 0)
{
    for (unsigned int i = 0; i < BOGEY_NUM_EYES; ++i) {
        mRay[i] = std::make_shared<Ray>(this, radius);
    }
    mFlock = 0;
    mFlocking = true;
    rotation = RAD(rand() % 360);
    calcRotationPoints(&rx, &ry, rotation, radius);
    setState(ALIVE);
    num_alive++;
    r = g = 0, b = 1;
    if (BOGEY == -1) {
        BOGEY = glGenLists(1);
        glNewList(BOGEY, GL_COMPILE);
        GLUquadricObj* m = gluNewQuadric();
        gluSphere(m, radius, radius + 3, 10);
        gluDeleteQuadric(m);
        glEndList();
    }
}

Bogey::~Bogey()
{
    for (unsigned int i = 0; i < BOGEY_NUM_EYES; ++i) {
        mRay[i] = nullptr;
    }
    num_alive--;
}

void Bogey::sync()
{
    if (!isAlive()) {
        return;
    }
    PhysicsObject::sync();
    calcRotationPoints(&rx, &ry, rotation, BOGEY_RADIUS);

    lx[0] = mPosition.x + rx * 2.5;
    ly[0] = mPosition.y + ry * 2.5;
    lx[1] = mPosition.x - rx * 2.5;
    ly[1] = mPosition.y - ry * 2.5;

    mRay[0]->set(lx[0], ly[0], mPosition.z);
    mRay[1]->set(lx[1], ly[1], mPosition.z);
}

extern bool mKill;

void Bogey::move(double dt)
{
    if (mKill && isAlive() && onScreen()) {
        setState(DYING);
        Global::audio->playSound(Audio::EXPLO_POP, pos);
        r = g = 0;
        b = 1;
        explosion.init(*this);
        for (unsigned int i = 0; i < BOGEY_NUM_EYES; ++i)
            mRay[i]->disable();
    }

    if (isAlive()) {
        shield.regenerate(dt);
        if (shield.getStrength() > 0) {
            radius = BOGEY_SHIELD_RADIUS;
        }

        if (onScreen() && Global::ship->isAlive()) {
            fire(dt);
        }

        if (mFlock) {
            FlockMember::move(dt);
        } else {
            rotate(((float)((rand() % 201) - 100) / 100.0) * dt);
            accelerate(((float)(rand() % 400) / 1000.0) * dt);
            ScreenObject::move(dt);
        }
    } else if (getState() == DYING) {
        if (explosion.finished) {
            setState(DEAD);
        } else {
            explosion.move(dt);
        }
    }
}

extern bool mDrawP;

void Bogey::accelerate(double amt)
{
    if (mDrawP) {
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
                        -sin(rotation + p) * mMaxSpeed * .5,
                        cos(rotation + p) * mMaxSpeed * .5,
                        sin(p) * mMaxSpeed * .5,
                        r, g, b, t);
        }
    }
    ScreenObject::accelerate(amt);
}

extern bool mDrawBLINE;

void Bogey::draw()
{
    float strength = shield.getStrength();
    r = 1 - strength;
    g = 0;
    b = strength / 1.25;

    if (getState() == ALIVE) {
        glPushMatrix();
        glTranslated(mPosition.x, -mPosition.y, mPosition.z);
        if (mDrawBLINE && !mFlock)
            draw::setColor(.2, .2, .2);
        else
            draw::setColor(r, g, b, 1.0);
        glCallList(BOGEY);
        glPopMatrix();
    } else if (getState() == DYING) {
        if (explosion.finished)
            setState(DEAD);
        explosion.draw();
    }
    if (mDrawBLINE) {
        static Coord3<double> ray_pos;
        draw::setColor(r, g, b, .5);
        for (unsigned int i = 0; i < BOGEY_NUM_EYES; ++i) {
            ray_pos.set(lx[i], ly[i], mPosition.z);
            draw::sphere(ray_pos, radius);
        }
    }
}

void Bogey::fire(double dt)
{
    static double rateOfFire = 150, fireCounter = rand() % 100;

    double ss = shield.getStrength() * 10;

    double directionToShip = atan2((double)(SX(Global::ship) - SX(this)),
                                   (double)(SY(this) - SY(Global::ship)));

    fireCounter += dt;
    if (fireCounter >= (rateOfFire + ss)) {
        fireCounter = 0;
    } else {
        return;
    }

    double error = 0;

    (void)new Missile(BOGEY_TYPE, directionToShip + error,
                      mPosition.x + (radius + 5) * sin(directionToShip + error),
                      mPosition.y - (radius + 5) * cos(directionToShip + error),
                      mPosition.z,
                      mVelocity.x, mVelocity.y, mVelocity.z);

    Global::audio->playSound(Audio::BOOM, pos);
}

void Bogey::rotate(double amt)
{
    ScreenObject::rotate(amt);
    calcRotationPoints(&rx, &ry, rotation, BOGEY_RADIUS);
}

void Bogey::damage(double amt)
{
    if (amt > shield.getStrength()) {
        if (isAlive()) {
            for (unsigned int i = 0; i < BOGEY_NUM_EYES; ++i)
                mRay[i]->disable();
            setState(DYING);
            disable();
            Global::audio->playSound(Audio::EXPLO_POP, pos);
            r = g = 0;
            b = .80;
            explosion.init(*this);
        }
    } else {
        shield.damage(amt);
        if (shield.getStrength() == 0) {
            radius = BOGEY_RADIUS;
        }
    }
}

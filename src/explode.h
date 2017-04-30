// --------------------------------------------------------------------------
//
// Copyright (c) 2003 Thomas D. Marsh. All rights reserved.
//
// "SSC" is free software; you can redistribute it
// and/or use it and/or modify it under the terms of
// the "GNU General Public License" (GPL).
//
// --------------------------------------------------------------------------

#ifndef SSC_EXPLODE_H
#define SSC_EXPLODE_H

#include "common.h"
#include "draw.h"
#include "object.h"
#include "screen.h"

const float LIFE_TIME = 240;
const unsigned int TRIANGLE_RADIUS = 10;

namespace Global {
extern Coord3<double> normal;
}

struct ExploTriangle {
    ExploTriangle() {}

    double radius;
    Coord3<double> mPosition, mForce, mRotation, mRotationSpeed;

    inline void draw()
    {
        glPushMatrix();
        glTranslatef(mPosition.x, -mPosition.y, mPosition.z);
        glRotated(mRotation.x, 1, 0, 0);
        glRotated(mRotation.y, 0, 1, 0);
        glRotated(mRotation.z, 0, 0, 1);

        draw::startTriangles();
        draw::trianglePoint(radius, 0, 0);
        draw::trianglePoint(0, -radius / 2, 0);
        draw::trianglePoint(0, radius / 2, 0);
        draw::endTriangles();
        glPopMatrix();
    }

    inline void move(double dt)
    {
        mPosition += mForce * dt;
        mRotation += mRotationSpeed * dt;
    }
};

template <int NUM_TRIANGLES>
class Explosion {
public:
    Explosion()
        : finished(false)
        , age(0)
    {
    }
    bool finished;
    float age;
    float r, g, b;

    void init(ScreenObject& obj)
    {
        r = obj.r;
        g = obj.g;
        b = obj.b;
        for (unsigned int i = 0; i < NUM_TRIANGLES; i++) {
            int n = (rand() % obj.radius);
            double d = RAD(rand() % 360);

            triangles[i].radius = TRIANGLE_RADIUS;
            triangles[i].mPosition.set(
                obj.mPosition.x + n * sin(d),
                obj.mPosition.y + n * cos(d),
                obj.mPosition.z + n * (sin(d * d)));
            triangles[i].mForce.set(
                obj.mVelocity.x + ((float)((rand() % 101) - 50) / 10.0),
                obj.mVelocity.y + ((float)((rand() % 101) - 50) / 10.0),
                obj.mVelocity.z + ((float)((rand() % 101) - 50) / 10.0));
            triangles[i].mRotation.set(
                RAD(rand() % 360),
                RAD(rand() % 360),
                RAD(rand() % 360));
            triangles[i].mRotationSpeed.set(
                2 * RAD(rand() % 360),
                2 * RAD(rand() % 360),
                2 * RAD(rand() % 360));
        }
    }

    inline void move(double dt)
    {
        for (unsigned int i = 0; i < NUM_TRIANGLES; i++) {
            triangles[i].move(dt);
        }

        age += dt;
        if (age > LIFE_TIME) {
            finished = true;
        }
    }

    inline void draw()
    {
        glDisable(GL_LIGHTING);
        glDisable(GL_LIGHT0);
        glDisable(GL_LIGHT1);
        float a = 1.0 - age / LIFE_TIME;
        draw::setColor(r, g, b, a);
        for (unsigned int i = 0; i < NUM_TRIANGLES; i++) {
            triangles[i].draw();
        }
        glEnable(GL_LIGHTING);
        glEnable(GL_LIGHT0);
        glEnable(GL_LIGHT1);
    }

private:
    ExploTriangle triangles[NUM_TRIANGLES];
};

#endif // SSC_EXPLODE_H

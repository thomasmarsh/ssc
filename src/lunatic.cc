// --------------------------------------------------------------------------
//
// Copyright (c) 2003 Thomas D. Marsh. All rights reserved.
//
// "SSC" is free software; you can redistribute it
// and/or use it and/or modify it under the terms of
// the "GNU General Public License" (GPL).
//
// --------------------------------------------------------------------------

#include "lunatic.h"
#include "draw.h"
#include "game.h"
#include "global.h"

const unsigned int LUNATIC_MAX_SPEED = 7;
const unsigned int LUNATIC_RADIUS = 7;
const double LUNATIC_MASS = 50.0;

Lunatic::Lunatic()
    : ScreenObject(LUNATIC_TYPE,
                   LUNATIC_RADIUS,
                   LUNATIC_MASS,
                   LUNATIC_MAX_SPEED,
                   (double)(rand() % 500),
                   (double)(rand() % 500),
                   0,
                   0, 0, 0)
{
    r = .9, g = .9, b = .9;
    alpha = (double)(rand() % 70) / 100;
    dir = true;

    setState(ALIVE);
}

Lunatic::~Lunatic()
{
}

void Lunatic::rotate(double amt)
{
    rotation += amt;

    while (rotation > D_PI) {
        rotation -= D_PI;
    }
    while (rotation < 0) {
        rotation += D_PI;
    }
}

// --------------------------------------------------------------------------
//
// FUZZY CONTROLLER
//
// --------------------------------------------------------------------------

//
// SPEED
//

#if 0
#define FLOG(s) fprintf(stderr, s);
#define FLOG1(s, a) fprintf(stderr, s, a);
#define FLOG2(s, a, b) fprintf(stderr, s, a, b);
#define FLOG3(s, a, b, c) fprintf(stderr, s, a, b, c);
#else
#define FLOG(s)
#define FLOG1(s, a)
#define FLOG2(s, a, b)
#define FLOG3(s, a, b, c)
#endif

enum Speed {
    S_POS_FAST,
    S_POS_SLOW,
    S_ZERO,
    S_NEG_SLOW,
    S_NEG_FAST,
    S_LAST
};

void pprint(Speed s)
{
    switch (s) {
    case S_POS_FAST:
        FLOG("S_POS_FAST");
        break;
    case S_POS_SLOW:
        FLOG("S_POS_SLOW");
        break;
    case S_ZERO:
        FLOG("S_ZERO");
        break;
    case S_NEG_SLOW:
        FLOG("S_NEG_SLOW");
        break;
    case S_NEG_FAST:
        FLOG("S_NEG_FAST");
        break;
    case S_LAST:
        FLOG("S_LAST");
        break;
    }
}

typedef double SpeedMembership[S_LAST];
typedef double SpeedMbf[S_LAST][3];

SpeedMbf SPEED_MBF = {
    { -7, -7, -1.4 },
    { -7, -1.4, 0 },
    { -1.4, 0, 1.4 },
    { 0, 1.4, 7 },
    { 1.4, 7, 7 }
};

//
// ANGLE
//

enum Angle {
    A_POS_LARGE,
    A_POS_SMALL,
    A_ZERO,
    A_NEG_SMALL,
    A_NEG_LARGE,
    A_LAST
};

void pprint(Angle s)
{
    switch (s) {
    case A_POS_LARGE:
        FLOG("A_POS_LARGE");
        break;
    case A_POS_SMALL:
        FLOG("A_POS_SMALL");
        break;
    case A_ZERO:
        FLOG("A_ZERO");
        break;
    case A_NEG_SMALL:
        FLOG("A_NEG_SMALL");
        break;
    case A_NEG_LARGE:
        FLOG("A_NEG_LARGE");
        break;
    case A_LAST:
        FLOG("A_LAST");
        break;
    }
}
typedef double AngleMembership[A_LAST];
typedef double AngleMbf[A_LAST][3];

AngleMbf ANGLE_MBF = {
    { RAD(0), RAD(0), RAD(144) },
    { RAD(0), RAD(144), RAD(180) },
    { RAD(144), RAD(180), RAD(216) },
    { RAD(180), RAD(216), RAD(360) },
    { RAD(216), RAD(360), RAD(360) }
};

//
// POWER
//

enum Power {
    P_POS_HI,
    P_POS_LOW,
    P_ZERO,
    P_NEG_LOW,
    P_NEG_HI,
    P_LAST
};

void pprint(Power p)
{
    switch (p) {
    case P_POS_HI:
        FLOG("P_POS_HI");
        break;
    case P_POS_LOW:
        FLOG("P_POS_LOW");
        break;
    case P_ZERO:
        FLOG("P_ZERO");
        break;
    case P_NEG_LOW:
        FLOG("P_NEG_LOW");
        break;
    case P_NEG_HI:
        FLOG("P_NEG_HI");
        break;
    case P_LAST:
        FLOG("P_LAST");
        break;
    }
}

double PowerPeak(Power p)
{
    switch (p) {
    case P_POS_HI:
        return 0;
    case P_POS_LOW:
        return .25;
    case P_ZERO:
        return .50;
    case P_NEG_LOW:
        return .75;
    case P_NEG_HI:
        return 1;
    default:
        break;
    }
    return 0;
}

struct Rule {
    Speed speed;
    Angle angle;
    Power power;
};

Rule RULES[] = {
    { S_POS_FAST, A_NEG_LARGE, P_POS_LOW },
    { S_POS_FAST, A_NEG_SMALL, P_NEG_LOW },
    { S_POS_FAST, A_ZERO, P_NEG_HI },
    { S_POS_FAST, A_POS_SMALL, P_NEG_HI },
    { S_POS_FAST, A_POS_LARGE, P_NEG_HI },

    { S_POS_SLOW, A_NEG_LARGE, P_POS_HI },
    { S_POS_SLOW, A_NEG_SMALL, P_ZERO },
    { S_POS_SLOW, A_ZERO, P_NEG_LOW },
    { S_POS_SLOW, A_POS_SMALL, P_NEG_HI },
    { S_POS_SLOW, A_POS_LARGE, P_NEG_HI },

    { S_ZERO, A_NEG_LARGE, P_POS_HI },
    { S_ZERO, A_NEG_SMALL, P_POS_LOW },
    { S_ZERO, A_ZERO, P_ZERO },
    { S_ZERO, A_POS_SMALL, P_NEG_LOW },
    { S_ZERO, A_POS_LARGE, P_NEG_HI },

    { S_NEG_SLOW, A_NEG_LARGE, P_POS_HI },
    { S_NEG_SLOW, A_NEG_SMALL, P_POS_HI },
    { S_NEG_SLOW, A_ZERO, P_POS_LOW },
    { S_NEG_SLOW, A_POS_SMALL, P_ZERO },
    { S_NEG_SLOW, A_POS_LARGE, P_NEG_HI },

    { S_NEG_FAST, A_NEG_LARGE, P_POS_HI },
    { S_NEG_FAST, A_NEG_SMALL, P_POS_HI },
    { S_NEG_FAST, A_ZERO, P_POS_HI },
    { S_NEG_FAST, A_POS_SMALL, P_POS_LOW },
    { S_NEG_FAST, A_POS_LARGE, P_NEG_LOW },
};

inline double membership(double tri[], double value)
{
    if (value == tri[1]) {
        return 1;
    }
    if (value < tri[0] || value > tri[2]) {
        return 0;
    }

    if (value < tri[1]) {
        return (1 / (tri[1] - tri[0])) * (value - tri[0]);
    }
    return (1 / (tri[2] - tri[1])) * (tri[2] - value);
}

inline double min(double a, double b)
{
    if (a < b) {
        return a;
    }
    return b;
}

double RunFuzzy(double angle, double speed)
{
    // normalize angle
    while (angle < 0) {
        angle += D_PI;
    }
    while (angle > D_PI) {
        angle -= D_PI;
    }

    // normalize speed
    if (speed > 7)
        speed = 7;
    if (speed < -7)
        speed = -7;

    //      FLOG("--------------------\n\n");
    double MAX = 0;
    double SM = 0, AM = 0;
    int best_r = 0;
    double squares[P_LAST];
    for (int i = 0; i < P_LAST; i++) {
        squares[i] = 0;
    }

    Power P;
    for (int i = 0, r = 0; i < S_LAST; i++) {
        Speed S = (Speed)i;
        for (int j = 0; j < A_LAST; j++, r++) {
            Angle A = (Angle)j;

            double sm = membership(SPEED_MBF[S], speed);
            double am = membership(ANGLE_MBF[A], angle);
            double memb = min(sm, am);

            P = RULES[i * j].power;
            squares[P] += min(sm, am); //PowerPeak(P);

            if (memb >= MAX) {
                MAX = memb;
                best_r = r;
                SM = sm;
                AM = am;
            }

            //                      FLOG2("RULE %d: M=%.2f: ", r+1, memb);
            //                      pprint(S);
            //                      FLOG(":");
            //                      pprint(A);
            //                      FLOG2(" = (%.2f, %.2f)\n", sm, am);
        }
        //              FLOG("\n");
    }
    double output = 0;
    double div = 0;
    for (int i = 0; i < P_LAST; i++) {
        P = (Power)i;
        //              FLOG("add power: "); pprint(P); FLOG("\n");
        //              FLOG3("output = %.2f + %.2f + %.2f\n", output, PowerPeak(P), squares[i]);
        //              FLOG2("div    = %.2f + %.2f\n", div, squares[i]);
        squares[i] = sqrt(squares[i]);
        output += PowerPeak(P) * squares[i];
        div += squares[i];
    }
    FLOG3("output = %.2f / %.2f = %.2f\n", output, div, output / div);
    output /= div;

    //      double fz_power = sqrt(squares[P]);
    //      fprintf(stderr, "%.2f -> %.2f\n", squares[P], output);

    //      fprintf(stderr, "best=%d: %.2f: ", best_r+1, MAX);
    //      pprint(RULES[best_r].speed);
    //      fprintf(stderr, ":");
    //      pprint(RULES[best_r].angle);
    //      fprintf(stderr, ":");
    //      pprint(P);
    //      fprintf(stderr, "\n");

    //      fprintf(stderr, "output=%.2f\n", output);
    //
    return output;
}

double calcSign(Coord3<double>& pos, Coord3<double>& vel)
{
    double d;
    Coord3<double> p2;

    p2 = pos + vel;

    d = distance(pos, Global::ship->mPosition);

    if (distance(p2, Global::ship->mPosition) > d) {
        return -1;
    }
    return 1;
}

void Lunatic::move(double dt)
{
    if (isAlive()) {
        if (dir) {
            alpha -= .01 * dt;
            if (alpha <= 0) {
                alpha = 0, dir = !dir;
            }
        } else {
            alpha += .01 * dt;
            if (alpha >= .7) {
                alpha = .7, dir = !dir;
            }
        }

        if (onScreen() && Global::ship->isAlive()) {
            if (speed > mMaxSpeed) {
                decelerate(dt);
                ScreenObject::move(dt);
                return;
            }

            static Coord3<double> mDisplace;

            //                      double d = distance(mPosition, Global::ship->mPosition);
            double output = 1;
            mDisplace = Global::ship->mPosition - mPosition;
            //                      if (d < 100)
            {

                double sign = calcSign(mPosition, mVelocity);

                double sr = Global::ship->mVelocity.angle();
                rotation = mVelocity.angle();
                double output = RunFuzzy(sr - rotation, sign * speed);
                if (sign < 0) {
                    output = 2;
                } else {
                    output = fabs((output + 1) / 2);
                }
            }

            rotation = atan2(mDisplace.x, -mDisplace.y);
            accelerate(output * dt);
        } else {
            rotate((float)((rand() % 201) - 100) / 100.0);
            accelerate((float)(rand() % 400) / 1000.0);
        }
    } else if (isDying()) {
        if (mExplosion.finished) {
            setState(DEAD);
        }
        mExplosion.move(dt);
    }
    ScreenObject::move(dt);
}

bool Lunatic::collision(ScreenObject& obj)
{
    static float pos[3] = { 0.0, 0.0, 0.0 };

    if ((obj.type() == MISSILE_TYPE) && (obj.ownerType() == PLAYER_TYPE)) {
        setState(DYING);
        Global::audio->playSound(Audio::EXPLO_POP, pos);
        mExplosion.init(*this);
    }
    double dt = Game::getInstance().dt;
    PhysicsObject& pobj = obj;
    pobj.accelerate(-pobj.mVelocity.x * dt * 2, -pobj.mVelocity.y * dt * 2, 0);
    return false;
}

void Lunatic::draw()
{
    if (isAlive()) {
        draw::setColor(r, g, b, alpha);
        draw::sphere(mPosition, radius);
    } else if (isDying()) {
        mExplosion.draw();
    }
}

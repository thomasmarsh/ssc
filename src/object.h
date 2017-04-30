// --------------------------------------------------------------------------
//
// Copyright (c) 2003 Thomas D. Marsh. All rights reserved.
//
// "SSC" is free software; you can redistribute it
// and/or use it and/or modify it under the terms of
// the "GNU General Public License" (GPL).
//
// --------------------------------------------------------------------------

#ifndef SSC_OBJECT_H
#define SSC_OBJECT_H

#include "screen.h"
#include "physics.h"


// --------------------------------------------------------------------------
//
// CLASS: ScreenObject
//
// ScreenObject is the base class for all on-screen objects which appear
// in the game and provides the following behaviours:
//
//      - collision detection and other physics
//      - typing (ObjectType)
//      - parent typing (virtual)
//      - statefulness (alive/dead)
//
// --------------------------------------------------------------------------

class ScreenObject : public PhysicsObject
{
public:
        // ------------------------------------------------------------------
        //
        // ENUM TYPES
        //
        // ------------------------------------------------------------------
        friend class Model;
        friend class Flock;
        friend class HUD;

        enum ObjectType
        {
                PLAYER_TYPE,
                MISSILE_TYPE,
                BOGEY_TYPE,
                FATSO_TYPE,
                LUNATIC_TYPE,
                BLACKHOLE_TYPE,
                ASTEROID_TYPE,
                SMARTY_TYPE
        };


        enum ObjectState
        {
                CREATE,
                ALIVE,
                DYING,
                DEAD
        };

        inline bool isAlive() { return mState == ALIVE; }
        inline bool isDead() { return mState == DEAD; }
        inline bool isDying() { return mState == DYING; }

        inline ScreenObject* getAddress() { return this; }

        // ------------------------------------------------------------------
        //
        // STATIC VARS
        //
        // ------------------------------------------------------------------

        virtual void behave();
        virtual void interact(ScreenObject &) {}


        // ------------------------------------------------------------------
        //
        // CTOR & DTOR
        //
        // ------------------------------------------------------------------

        ScreenObject(ObjectType t,
                     int radius, double mass, double ms,
                     double x, double y, double z=0,
                     double fx=0, double fy=0, double fz=0);

        virtual ~ScreenObject();

        
        // ------------------------------------------------------------------
        //
        // ACCESSORS AND SETTERS
        //
        // ------------------------------------------------------------------

        // accessors
        inline ObjectType type() { return mType; }
        inline ObjectState getState() { return mState; }
        virtual ObjectType ownerType() { return mType; }

        // setters
        inline virtual void setState(ObjectState state)
        {
                mState = state;
                if (mState == DYING) disable();
        }


public:
        // ------------------------------------------------------------------
        //
        // MOTION AND ROTATION
        //
        // ------------------------------------------------------------------

        bool onScreen();

        virtual void accelerate(double amt);
        virtual void decelerate(double dt);
        virtual void move(double dt);
        virtual void rotate(double dir);

public:

        // ------------------------------------------------------------------
        //
        // MEMBER VARIABLES
        //
        // ------------------------------------------------------------------

        float r, g, b;          // color of this object

        double rotation;        // rotation in radians
        int radius;             // draw radius

        bool mFlocking;         // whether this object participates in flocking

protected:
        bool mGravity;
        Coord2<double>  mGravityAffect;

protected:
        ObjectType mType;
        ObjectState mState;
        double mMaxSpeed;

public:
        // ------------------------------------------------------------------
        //
        // COLLISION DETECTION
        //
        // ------------------------------------------------------------------

        virtual void reset();

public:
        inline bool shouldCollide(Collidable *other)
        {
                if (!isAlive()) return false;

                if (!other) return true;

                const CollisionData *data = other->collisionData();

                if (!data) return true;

                switch (data->type)
                {
                case COLLISION_WALL:
                        return true;
                case COLLISION_SCREENOBJECT:
                        return collision(*((ScreenObject*) (data->ptr)));
                case COLLISION_RAY:
                        return false;
                }
                return true;
        }

        virtual bool collision(ScreenObject& obj) = 0;


        // ------------------------------------------------------------------
        //
        // CONTRACTUAL REQUIREMENTS
        //
        // ------------------------------------------------------------------

        virtual void draw() = 0;

        ScreenObject *next, *prev;
        bool mDecelFlag;
};

#endif // SSC_OBJECT_H
